#define _POSIX_C_SOURCE 200809L

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void)
{
    int log_fd = open("launcher.log", O_WRONLY | O_CREAT | O_APPEND, 0600);
    if (log_fd == -1) {
        perror("open");
        return 1;
    }

    if (dprintf(log_fd, "Starting worker\n") < 0) {
        perror("write log");
        close(log_fd);
        return 1;
    }

    pid_t child = fork();
    if (child == -1) {
        perror("fork");
        close(log_fd);
        return 1;
    }

    if (child == 0) {
        execlp("printf", "printf", "%s\n", "Worker finished", (char *)NULL);
        perror("exec printf");
        _exit(127);
    }

    /* The parent no longer needs the log. */
    int close_failed = close(log_fd) == -1;
    if (close_failed) {
        perror("close log");
    }

    int status;
    while (waitpid(child, &status, 0) == -1) {
        if (errno != EINTR) {
            perror("waitpid");
            return 1;
        }
    }

    if (close_failed || !WIFEXITED(status)) {
        return 1;
    }
    return WEXITSTATUS(status);
}
