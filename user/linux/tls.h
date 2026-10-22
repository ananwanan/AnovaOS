/* SPDX-License-Identifier: Apache-2.0 */
/* Share the strict user-space HTTPS fetch contract with package installation and TLS probes. */
#ifndef ANOVA_TLS_H
#define ANOVA_TLS_H
#include <stddef.h>
int https_get(const char *hostname, unsigned port, const char *ca_path, const char *path,
              unsigned char *output, size_t capacity, size_t *length);
#endif
