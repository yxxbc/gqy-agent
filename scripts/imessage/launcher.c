// iMessage 桥接的专用启动器。
//
// 完全磁盘访问权限只授给这个二进制:macOS 按「负责进程」判定权限,子进程
// 继承负责进程,所以由它拉起的 Python 能读 chat.db,而同一个 Python 被别的
// 途径启动时拿不到这份权限。
//
// 解释器与脚本路径在编译时写死(install.sh 传 -D),不接受命令行指定程序,
// 避免它被当成「任意程序都能借用的读盘跳板」。
#include <errno.h>
#include <signal.h>
#include <spawn.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

#ifndef BRIDGE_PYTHON
#error "BRIDGE_PYTHON must be defined at compile time"
#endif
#ifndef BRIDGE_SCRIPT
#error "BRIDGE_SCRIPT must be defined at compile time"
#endif

extern char **environ;

static pid_t child = -1;

static void forward_signal(int sig) {
    if (child > 0) {
        kill(child, sig);
    }
}

int main(void) {
    signal(SIGTERM, forward_signal);
    signal(SIGINT, forward_signal);
    signal(SIGHUP, forward_signal);

    char exe[4096];
    uint32_t size = sizeof(exe);
    // 让脚本在报错里说出该给谁授权
    extern int _NSGetExecutablePath(char *buf, uint32_t *bufsize);
    if (_NSGetExecutablePath(exe, &size) == 0) {
        char resolved[4096];
        if (realpath(exe, resolved) != NULL) {
            setenv("GQY_IMESSAGE_LAUNCHER", resolved, 1);
        }
    }

    char *const args[] = {BRIDGE_PYTHON, BRIDGE_SCRIPT, NULL};
    int rc = posix_spawn(&child, BRIDGE_PYTHON, NULL, NULL, args, environ);
    if (rc != 0) {
        fprintf(stderr, "gqy-imessage: spawn %s failed: %d\n", BRIDGE_PYTHON, rc);
        return 127;
    }

    int status;
    while (waitpid(child, &status, 0) < 0) {
        if (errno != EINTR) {
            return 1;
        }
    }
    if (WIFEXITED(status)) {
        return WEXITSTATUS(status);
    }
    return 128 + WTERMSIG(status);
}
