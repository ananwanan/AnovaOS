// SPDX-License-Identifier: Apache-2.0
// Verify guest-built C stdio, allocation, arguments, persistent I/O and multi-file linking.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
int helper(void);
int main(int argc, char **argv) {
    char *p = malloc(32);
    FILE *f = fopen("result.txt", "w");
    if (!p || !f || argc != 2 || strcmp(argv[1], "guest")) return 1;
    fprintf(f, "guest-C:%d\n", helper());
    if (fclose(f)) return 2;
    free(p);
    puts("[M11:C] stdio malloc files argv multi-source");
    return 0;
}
