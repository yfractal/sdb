#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>

// mkfifo /tmp/start_symbolizer
// gcc -shared -fPIC -o symbolizer_trigger.so symbolizer_trigger.c
// LD_PRELOAD=./symbolizer_trigger.so ruby ~/tmp/example.rb

__attribute__((constructor)) void pre_load() {
    const char *fifo_path = "/tmp/start_symbolizer";
    int fd = open(fifo_path, O_WRONLY);

    if (fd == -1) {
        perror("open");
        return;
    }


    pid_t process_id = getpid();
    char pid_str[20];
    int len = snprintf(pid_str, sizeof(pid_str), "%d", process_id);

    write(fd, pid_str, len);
    close(fd);

    printf("Current process ID: %d\n", process_id);
    printf("Preloaded code before any library is loaded!\n");
}
