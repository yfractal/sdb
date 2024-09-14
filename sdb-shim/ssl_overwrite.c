#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <assert.h>
#include <zlib.h>
#include "openssl/ssl.h"

#define CHUNK 16384

// compile:
// gcc -I/opt/homebrew/Cellar/openssl@1.1/1.1.1w/include -L/opt/homebrew/Cellar/openssl@1.1/1.1.1w/lib -lssl -lcrypto -shared -fPIC -o ssl_overwrite.dylib ssl_overwrite.c
struct __osx_interpose {
    const void* new_func;
    const void* orig_func;
};

static int Real__SSL_read (void *ssl, void *buf, int num) { return SSL_read (ssl, buf, num); }
extern int __interpose_SSL_read (void *ssl, void *buf, int num);

static const struct __osx_interpose __osx_interpose_SSL_read __attribute__((used, section("__DATA, __interpose"))) =
  { (const void*)((uintptr_t)(&(__interpose_SSL_read))),
    (const void*)((uintptr_t)(&(SSL_read))) };


// Function to read chunk size (hexadecimal) from the body
int read_chunk_size(const unsigned char *body, const unsigned char **rest) {
    int chunk_size = 0;
    sscanf((const char *)body, "%x", &chunk_size);

    // Move the body pointer to after the chunk size and the following CRLF
    const unsigned char *end_of_size = (const unsigned char *)strstr((const char *)body, "\r\n");
    if (end_of_size != NULL) {
        *rest = end_of_size + 2; // Skip CRLF
    } else {
        *rest = body;
    }

    return chunk_size;
}

// Function to skip CRLF
const unsigned char *skip_crlf(const unsigned char *body) {
    assert(strncmp((const char *)body, "\r\n", 2) == 0);
    return body + 2;
}

// Function to read the chunked HTTP body
unsigned char *read_chunked_http_body(const unsigned char *body, size_t body_len, size_t *out_len) {
    unsigned char *decoded_body = NULL;
    size_t decoded_size = 0;

    while (body_len > 0) {
        // Read chunk size
        const unsigned char *rest = body;
        int chunk_size = read_chunk_size(body, &rest);
        body_len -= (rest - body);
        body = rest;

        // If chunk size is 0, it's the end of the body
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

        // Copy the chunk data to the decoded body
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

    // Set the output length
    if (out_len) {
        *out_len = decoded_size;
    }

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

void print_bytes_as_hex(const unsigned char* data, size_t length) {
    fprintf(__stderrp, "[print_bytes_as_hex]\n");
    for (size_t i = 0; i < length; i++) {
        if (i > 0) {
            fprintf(__stderrp, " ");
        }
        fprintf(__stderrp, "%02x", data[i]);
    }

    fprintf(__stderrp, "\n");
}

int decompress_gzip(const unsigned char *compressed_data, size_t compressed_data_len, unsigned char *output, size_t output_len) {
    int ret;
    z_stream strm;
    memset(&strm, 0, sizeof(strm));  // Initialize z_stream structure

    // Initialize inflation (decompression)
    ret = inflateInit2(&strm, 16 + MAX_WBITS);  // 16 + MAX_WBITS to support gzip
    if (ret != Z_OK) {
        return ret;
    }

    strm.next_in = (unsigned char *)compressed_data;
    strm.avail_in = compressed_data_len;
    strm.next_out = output;
    strm.avail_out = output_len;

    // Perform the decompression
    ret = inflate(&strm, Z_NO_FLUSH);
    if (ret != Z_STREAM_END) {
        inflateEnd(&strm);
        return ret == Z_OK ? Z_BUF_ERROR : ret;
    }

    // Cleanup
    inflateEnd(&strm);
    return Z_OK;
}

extern int __interpose_SSL_read (void *ssl, void *buf, int num) {
  int ret = Real__SSL_read(ssl, buf, num);
  char* headers = NULL;
  char* raw_body = NULL;

  if (ret > 0) {
    fprintf(__stderrp, "[ssl_return]: ret len=%d, buf_len=%lu\n buf:\n", ret, strlen(buf));
    print_bytes_as_hex(buf, ret);
    int body_offset = http_body_offset(buf);
    fprintf(__stderrp, "body_offset: %d\n", body_offset);

    fprintf(__stderrp, "body bytes:\n");
    print_bytes_as_hex((const unsigned char *)(buf + body_offset), ret - body_offset);

    size_t decoded_len;
    unsigned char *decoded_body = read_chunked_http_body((const unsigned char *)(buf + body_offset), ret - body_offset, &decoded_len);

    if (decoded_body != NULL) {
      fprintf(__stderrp, "decoded_body:\n");
      print_bytes_as_hex(decoded_body, decoded_len);

      // Buffer for the decompressed output (Make sure it's large enough for decompressed data)
      unsigned char decompressed_output[CHUNK];
      size_t output_len = CHUNK;

      // Decompress the gzip data
      int ret = decompress_gzip(decoded_body, ret - body_offset, decompressed_output, output_len);
      if (ret == Z_OK) {
          printf("Decompressed body: %s\n", decompressed_output);
      } else {
          printf("Decompression failed with error code: %d\n", ret);
      }

      free(decoded_body);
    }
  }

  return ret;
}