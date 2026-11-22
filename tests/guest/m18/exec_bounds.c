// SPDX-License-Identifier: Apache-2.0
// Exercise Linux exec admission and exact startup bytes with real guest child processes.
// The shell case models the measured upstream all-gcc recipe's single -c operand.
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

enum {
    STRING_BYTES = 12287,
    VECTOR_BYTES = 16 * 1024,
    VECTOR_ENTRIES = 512,
    STARTUP_BYTES = 32 * 1024,
    // Four NULs, 16 random bytes, then argc, argv/envp, their sentinels and 14 aux pairs.
    STARTUP_METADATA = 4 + 16 + (4 + 3 + 2 * 14) * 8,
    STARTUP_VECTOR_BYTES = (STARTUP_BYTES - STARTUP_METADATA) / 2,
    RECURSIVE_RECIPE_BYTES = 8355,
    CHILD_MISMATCH = 91,
    CHILD_E2BIG = 92,
    CHILD_EXEC_FAILURE = 93,
};

extern char **environ;

static char first[STRING_BYTES + 2];
static char second[STRING_BYTES + 2];
static char third[STRING_BYTES + 2];
static char fourth[STRING_BYTES + 2];
static char *empty[] = {NULL};
static char string_case[] = "string";
static char argv_bytes_case[] = "argv-bytes";
static char env_bytes_case[] = "env-bytes";
static char entries_case[] = "entries";
static char entry[] = "entry";

static void payload(char *output, size_t length, char byte)
{
    memset(output, byte, length);
    output[length] = '\0';
}

static int matches(const char *string, size_t length, char byte)
{
    if (string == NULL || strlen(string) != length)
        return 0;
    for (size_t i = 0; i < length; i++) {
        if (string[i] != byte)
            return 0;
    }
    return 1;
}

static int verify_startup(int argc, char **argv)
{
    size_t envc = 0;
    while (environ[envc] != NULL)
        envc++;

    if (strcmp(argv[0], string_case) == 0)
        return argc == 2 && envc == 1 && matches(argv[1], STRING_BYTES, 'a')
            && matches(environ[0], STRING_BYTES, 'b');
    if (strcmp(argv[0], argv_bytes_case) == 0)
        return argc == 3 && envc == 0 && matches(argv[1], STRING_BYTES, 'a')
            && matches(argv[2], VECTOR_BYTES - STRING_BYTES - strlen(argv[0]), 'b');
    if (strcmp(argv[0], env_bytes_case) == 0)
        return argc == 1 && envc == 2 && matches(environ[0], STRING_BYTES, 'a')
            && matches(environ[1], VECTOR_BYTES - STRING_BYTES, 'b');
    if (strcmp(argv[0], entries_case) == 0) {
        if (argc != VECTOR_ENTRIES || envc != VECTOR_ENTRIES)
            return 0;
        for (size_t i = 0; i < VECTOR_ENTRIES; i++) {
            if ((i != 0 && strcmp(argv[i], entry) != 0) || strcmp(environ[i], entry) != 0)
                return 0;
        }
        return 1;
    }
    return argc == 2 && envc == 2 && matches(argv[0], STRING_BYTES, 'A')
        && matches(argv[1], STARTUP_VECTOR_BYTES - STRING_BYTES, 'b')
        && matches(environ[0], STRING_BYTES, 'c')
        && matches(environ[1], STARTUP_VECTOR_BYTES - STRING_BYTES, 'd');
}

static void expect_exec(const char *label, const char *path, char *const argv[],
    char *const envp[], int expected)
{
    pid_t pid = fork();
    if (pid < 0) {
        perror("fork");
        exit(1);
    }
    if (pid == 0) {
        execve(path, argv, envp);
        if (errno == E2BIG)
            _exit(CHILD_E2BIG);
        perror("execve");
        _exit(CHILD_EXEC_FAILURE);
    }
    int status;
    if (waitpid(pid, &status, 0) != pid) {
        perror("waitpid");
        exit(1);
    }
    if (!WIFEXITED(status) || WEXITSTATUS(status) != expected) {
        fprintf(stderr, "[M18:GCC:EXEC] %s wait_status=%d expected_exit=%d\n",
            label, status, expected);
        exit(1);
    }
    printf("[M18:GCC:EXEC] %s %s\n", label, expected == CHILD_E2BIG ? "E2BIG" : "accepted");
}

static void check_strings(const char *self)
{
    char *args[] = {string_case, first, NULL};
    char *env[] = {second, NULL};
    payload(first, STRING_BYTES, 'a');
    payload(second, STRING_BYTES, 'b');
    expect_exec("string-exact", self, args, env, 0);
    payload(first, STRING_BYTES + 1, 'a');
    expect_exec("argv-string-overflow", self, args, env, CHILD_E2BIG);
    payload(first, STRING_BYTES, 'a');
    payload(second, STRING_BYTES + 1, 'b');
    expect_exec("env-string-overflow", self, args, env, CHILD_E2BIG);
}

static void check_vectors(const char *self)
{
    char *args[] = {argv_bytes_case, first, second, NULL};
    char *env[] = {first, second, NULL};
    char *env_args[] = {env_bytes_case, NULL};
    const size_t argv_tail = VECTOR_BYTES - STRING_BYTES - strlen(argv_bytes_case);
    const size_t env_tail = VECTOR_BYTES - STRING_BYTES;
    payload(first, STRING_BYTES, 'a');
    payload(second, argv_tail, 'b');
    expect_exec("argv-bytes-exact", self, args, empty, 0);
    payload(second, argv_tail + 1, 'b');
    expect_exec("argv-bytes-overflow", self, args, empty, CHILD_E2BIG);
    payload(second, env_tail, 'b');
    expect_exec("env-bytes-exact", self, env_args, env, 0);
    payload(second, env_tail + 1, 'b');
    expect_exec("env-bytes-overflow", self, env_args, env, CHILD_E2BIG);

    char *many_args[VECTOR_ENTRIES + 2] = {entries_case};
    char *many_env[VECTOR_ENTRIES + 2] = {NULL};
    for (size_t i = 0; i < VECTOR_ENTRIES; i++) {
        if (i != 0)
            many_args[i] = entry;
        many_env[i] = entry;
    }
    expect_exec("entries-exact", self, many_args, many_env, 0);
    many_args[VECTOR_ENTRIES] = entry;
    expect_exec("argv-entries-overflow", self, many_args, many_env, CHILD_E2BIG);
    many_args[VECTOR_ENTRIES] = NULL;
    many_env[VECTOR_ENTRIES] = entry;
    expect_exec("env-entries-overflow", self, many_args, many_env, CHILD_E2BIG);
}

static void check_startup(const char *self)
{
    const size_t tail = STARTUP_VECTOR_BYTES - STRING_BYTES;
    char *args[] = {first, second, NULL};
    char *env[] = {third, fourth, NULL};
    payload(first, STRING_BYTES, 'A');
    payload(second, tail, 'b');
    payload(third, STRING_BYTES, 'c');
    payload(fourth, tail, 'd');
    expect_exec("startup-exact", self, args, env, 0);
    payload(fourth, tail + 1, 'd');
    expect_exec("startup-overflow", self, args, env, CHILD_E2BIG);
}

int main(int argc, char **argv)
{
    if (argc == 0)
        return CHILD_MISMATCH;
    if (argv[0][0] != '/')
        return verify_startup(argc, argv) ? 0 : CHILD_MISMATCH;

    check_strings(argv[0]);
    check_vectors(argv[0]);
    check_startup(argv[0]);

    char command[] = "exit 43";
    char *shell_args[] = {"/bootstrap/sh", "-c", first, NULL};
    payload(first, RECURSIVE_RECIPE_BYTES, ' ');
    memcpy(first + RECURSIVE_RECIPE_BYTES - strlen(command), command, sizeof(command));
    expect_exec("8355-byte-shell-operand", shell_args[0], shell_args, empty, 43);
    puts("[M18:GCC:EXEC] success");
    return 0;
}
