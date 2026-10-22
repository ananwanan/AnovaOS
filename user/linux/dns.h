/* SPDX-License-Identifier: Apache-2.0 */
/* Expose a bounded IPv4 DNS A resolver for guest commands and controlled peer tests. */
#ifndef ANOVA_DNS_H
#define ANOVA_DNS_H
#include <stddef.h>
#include <stdint.h>
int dns_answer(const unsigned char *bytes, size_t length, uint16_t transaction,
               unsigned char ip[4]);
int dns_resolve(const char *name, const char *server, unsigned port, unsigned char ip[4]);
#endif
