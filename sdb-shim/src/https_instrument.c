#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <assert.h>
#include <zlib.h>
#include "openssl/ssl.h"
#include "sys/socket.h"

#define MAX_HTTP_FD_COUNT 100000

// compile:
//  gcc -I/opt/homebrew/Cellar/openssl@1.1/1.1.1w/include -L/opt/homebrew/Cellar/openssl@1.1/1.1.1w/lib -lssl -lcrypto -shared -fPIC -o https_instrument.dylib src/https_instrument.c -lz
struct __osx_interpose {
    const void* new_func;
    const void* orig_func;
};

int *HTTP_FDS = NULL;

int initialize_http_fds_once() {
    if (HTTP_FDS == NULL) {
        // when this line has been executed concurrently, some memory could be wated
        HTTP_FDS = (int *)malloc(MAX_HTTP_FD_COUNT * sizeof(int));

        if (HTTP_FDS == NULL) {
            fprintf(stderr, "Memory allocation failed\n");
            return -1;
        }

        for (int i = 0; i < MAX_HTTP_FD_COUNT; i++) {
            HTTP_FDS[i] = -1;
        }
    }

    return 0;
}

// for SSL_read
static int Real__SSL_read(void *ssl, void *buf, int num) { return SSL_read (ssl, buf, num); }
extern int __interpose_SSL_read(void *ssl, void *buf, int num);

static const struct __osx_interpose __osx_interpose_SSL_read __attribute__((used, section("__DATA, __interpose"))) =
  { (const void*)((uintptr_t)(&(__interpose_SSL_read))),
    (const void*)((uintptr_t)(&(SSL_read))) };

// for read
static ssize_t Real__read(int socket, void *buffer, size_t length) { return read(socket, buffer, length); }
extern ssize_t __interpose_read(int, void *, size_t);

static const struct __osx_interpose __osx_interpose_read __attribute__((used, section("__DATA, __interpose"))) =
  { (const void*)((uintptr_t)(&(__interpose_read))),
    (const void*)((uintptr_t)(&(read))) };

// socket
static int Real__socket(int domain, int type, int protocol) { return socket(domain, type, protocol); }
extern int __interpose_socket(int domain, int type, int protocol);

static const struct __osx_interpose __osx_interpose_socket __attribute__((used, section("__DATA, __interpose"))) =
  { (const void*)((uintptr_t)(&(__interpose_socket))),
    (const void*)((uintptr_t)(&(socket))) };

// close
static int Real__close(int fd) { return close(fd); }
extern int __interpose_close(int fd);

static const struct __osx_interpose __osx_interpose_close __attribute__((used, section("__DATA, __interpose"))) =
  { (const void*)((uintptr_t)(&(__interpose_close))),
    (const void*)((uintptr_t)(&(close))) };

const unsigned char *skip_crlf(const unsigned char *body) {
    assert(strncmp((const char *)body, "\r\n", 2) == 0);
    return body + 2;
}

// Read chunk size (hexadecimal) from the body
int read_chunk_size(const unsigned char *body, const unsigned char **rest) {
    int chunk_size = 0;
    sscanf((const char *)body, "%x", &chunk_size);

    // Move the body pointer to after the chunk size and the following CRLF
    const unsigned char *end_of_size = (const unsigned char *)strstr((const char *)body, "\r\n");
    if (end_of_size != NULL) {
        *rest = skip_crlf(end_of_size);
    } else {
        *rest = body;
    }

    return chunk_size;
}

// Read the chunked HTTP body
unsigned char *read_chunked_http_body(const unsigned char *body, size_t body_len, size_t *out_len) {
    unsigned char *decoded_body = NULL;
    size_t decoded_size = 0;

    while (body_len > 0) {
        const unsigned char *rest = body;
        int chunk_size = read_chunk_size(body, &rest);
        body_len -= (rest - body);
        body = rest;

        // chunk size is 0, end of the body
        if (chunk_size == 0) {
            break;
        }

        // Ensure we have enough data for the chunk
        if (body_len < chunk_size) {
            fprintf(stderr, "Insufficient data for chunk\n");
            free(decoded_body);
            return NULL;
        }

        // Allocate or reallocate buffer for decoded data
        decoded_body = realloc(decoded_body, decoded_size + chunk_size);
        if (decoded_body == NULL) {
            fprintf(stderr, "Failed to allocate memory\n");
            return NULL;
        }

        memcpy(decoded_body + decoded_size, body, chunk_size);
        decoded_size += chunk_size;
        body += chunk_size;
        body_len -= chunk_size;

        // Skip the CRLF after the chunk
        if (body_len < 2 || strncmp((const char *)body, "\r\n", 2) != 0) {
            fprintf(stderr, "Missing CRLF after chunk\n");
            free(decoded_body);
            return NULL;
        }
        body = skip_crlf(body);
        body_len -= 2;
    }

    *out_len = decoded_size;

    return decoded_body;
}

int http_body_offset(const char* http_str) {
    const char* delimiter = "\r\n\r\n";
    char* split_pos = strstr(http_str, delimiter);

    if (split_pos != NULL) {
        int offset = split_pos - http_str + strlen(delimiter);
        return offset;
    } else {
        return -1;
    }
}

int decompress_gzip(const unsigned char *compressed_data, size_t compressed_data_len, unsigned char *output, size_t output_len) {
    int ret;
    z_stream strm;
    memset(&strm, 0, sizeof(strm));

    ret = inflateInit2(&strm, 16 + MAX_WBITS);
    if (ret != Z_OK) {
        return ret;
    }

    strm.next_in = (unsigned char *)compressed_data;
    strm.avail_in = compressed_data_len;
    strm.next_out = output;
    strm.avail_out = output_len;

    // decompress
    ret = inflate(&strm, Z_NO_FLUSH);
    if (ret != Z_STREAM_END) {
        inflateEnd(&strm);
        return ret == Z_OK ? Z_BUF_ERROR : ret;
    }

    // Cleanup
    inflateEnd(&strm);
    return Z_OK;
}

void print_buffer(const char* buff, size_t len) {
    for (size_t i = 0; i < len; i++) {
        printf("%c", buff[i]);
    }
}

extern int __interpose_SSL_read (void *ssl, void *buf, int num) {
  int ret = Real__SSL_read(ssl, buf, num);
  char* headers = NULL;
  char* raw_body = NULL;

  if (ret > 0) {
    int body_offset = http_body_offset(buf);

    print_buffer(buf, body_offset - 1);

    size_t decoded_len;
    unsigned char *decoded_body = read_chunked_http_body((const unsigned char *)(buf + body_offset), ret - body_offset, &decoded_len);

    if (decoded_body != NULL) {
      // Buffer for the decompressed output
      // TODO: Make sure it's large enough for decompressed data
      size_t alloated_size = 16384;
      unsigned char decompressed_output[alloated_size];
      size_t output_len = alloated_size;

      int ret = decompress_gzip(decoded_body, ret - body_offset, decompressed_output, output_len);
      assert(output_len <= alloated_size);

      if (ret == Z_OK) {
          printf("\n\n%s\n", decompressed_output);
      } else {
          printf("Decompression failed with error code: %d\n", ret);
      }

      free(decoded_body);
    }
  }

  return ret;
}

extern ssize_t __interpose_read(int socket, void *buffer, size_t length) {
    ssize_t ret = Real__read(socket, buffer, length);
    if (ret > 0 && HTTP_FDS != NULL && HTTP_FDS[socket] == 1) {
        print_buffer(buffer, ret);
        printf("\n\n");
    }

    return ret;
}

extern int __interpose_socket(int domain, int type, int protocol) {
    int ret = Real__socket(domain, type, protocol);

    // todo: handle concurrency issues
    if (domain == AF_INET || domain == AF_INET6) {
        initialize_http_fds_once();
        HTTP_FDS[ret] = 1;
    }

    return ret;
}

extern int __interpose_close(int fd) {
    int ret = Real__close(fd);
    // todo: handle concurrency issues
    if (ret == 0 && HTTP_FDS != NULL && fd > 0) {
        HTTP_FDS[fd] = -1;
    }
    return ret;
}
