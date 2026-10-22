/* SPDX-License-Identifier: Apache-2.0 */
/* Fetch bounded HTTPS bodies using pinned Mbed TLS with mandatory CA, hostname and date validation.
 */
#define _GNU_SOURCE
#include "tls.h"
#include <arpa/inet.h>
#include <errno.h>
#include <mbedtls/ssl.h>
#include <mbedtls/x509_crt.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/random.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <time.h>
#include <unistd.h>
static uint32_t verification_flags;
static int handshake_error;
static int random_bytes(void *ctx, unsigned char *out, size_t n) {
    (void)ctx;
    while (n) {
        ssize_t k = getrandom(out, n, 0);
        if (k <= 0)
            return -1;
        out += k;
        n -= (size_t)k;
    }
    return 0;
}
static int send_bytes(void *ctx, const unsigned char *b, size_t n) {
    ssize_t k = send(*(int *)ctx, b, n, MSG_NOSIGNAL);
    return k < 0 ? MBEDTLS_ERR_SSL_INTERNAL_ERROR : (int)k;
}
static int receive_bytes(void *ctx, unsigned char *b, size_t n) {
    ssize_t k = recv(*(int *)ctx, b, n, 0);
    return k < 0 ? MBEDTLS_ERR_SSL_INTERNAL_ERROR : (int)k;
}
int https_get(const char *hostname, unsigned port, const char *ca_path, const char *path,
              unsigned char *output, size_t capacity, size_t *length) {
    int result = -1, fd = -1;
    verification_flags = 0;
    handshake_error = 0;
    if (port == 0 || port > 65535)
        return -1;
    size_t ca_len = 0;
    unsigned char ca_bytes[8192];
    mbedtls_ssl_context ssl;
    mbedtls_ssl_config config;
    mbedtls_x509_crt ca;
    mbedtls_ssl_init(&ssl);
    mbedtls_ssl_config_init(&config);
    mbedtls_x509_crt_init(&ca);
    FILE *file = fopen(ca_path, "rb");
    if (!file)
        goto done;
    ca_len = fread(ca_bytes, 1, sizeof(ca_bytes) - 1, file);
    int bad = ferror(file) || !feof(file);
    fclose(file);
    if (bad || ca_len == 0)
        goto done;
    ca_bytes[ca_len] = 0;
    if (time(0) < 1577836800) {
        puts("[TLS:REJECT] trustworthy wall time unavailable");
        goto done;
    }
    if (mbedtls_x509_crt_parse(&ca, ca_bytes, ca_len + 1))
        goto done;
    if (mbedtls_ssl_config_defaults(&config, MBEDTLS_SSL_IS_CLIENT, MBEDTLS_SSL_TRANSPORT_STREAM,
                                    MBEDTLS_SSL_PRESET_DEFAULT))
        goto done;
    mbedtls_ssl_conf_authmode(&config, MBEDTLS_SSL_VERIFY_REQUIRED);
    mbedtls_ssl_conf_ca_chain(&config, &ca, 0);
    mbedtls_ssl_conf_rng(&config, random_bytes, 0);
    mbedtls_ssl_conf_min_version(&config, MBEDTLS_SSL_MAJOR_VERSION_3, MBEDTLS_SSL_MINOR_VERSION_3);
    if (mbedtls_ssl_setup(&ssl, &config) || mbedtls_ssl_set_hostname(&ssl, hostname))
        goto done;
    fd = socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0)
        goto done;
    struct sockaddr_in address = {.sin_family = AF_INET, .sin_port = htons(port)};
    inet_pton(AF_INET, "10.0.2.2", &address.sin_addr);
    struct timeval timeout = {.tv_sec = 5};
    if (setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof timeout) ||
        setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, sizeof timeout) ||
        connect(fd, (void *)&address, sizeof address))
        goto done;
    mbedtls_ssl_set_bio(&ssl, &fd, send_bytes, receive_bytes, 0);
    int error = mbedtls_ssl_handshake(&ssl);
    verification_flags = mbedtls_ssl_get_verify_result(&ssl);
    handshake_error = error;
    if (error || verification_flags) {
        printf("[TLS:REJECT] handshake=%d verify=%lu\n", error,
               (unsigned long)mbedtls_ssl_get_verify_result(&ssl));
        goto done;
    }
    char request[512];
    int request_len =
        snprintf(request, sizeof request,
                 "GET %s HTTP/1.0\r\nHost: %s\r\nConnection: close\r\n\r\n", path, hostname);
    if (request_len < 0 || request_len >= (int)sizeof request)
        goto done;
    size_t sent = 0;
    while (sent < (size_t)request_len) {
        int k =
            mbedtls_ssl_write(&ssl, (unsigned char *)request + sent, (size_t)request_len - sent);
        if (k <= 0)
            goto done;
        sent += (size_t)k;
    }
    char header[2048];
    size_t count = 0;
    while (count + 1 < sizeof header) {
        int k = mbedtls_ssl_read(&ssl, (unsigned char *)header + count, 1);
        if (k <= 0)
            goto done;
        count++;
        if (count >= 4 && !memcmp(header + count - 4, "\r\n\r\n", 4))
            break;
    }
    header[count] = 0;
    if (count + 1 == sizeof header || strncmp(header, "HTTP/1.0 200 ", 13))
        goto done;
    char *field = strstr(header, "\r\nContent-Length: ");
    if (!field || strstr(header, "Transfer-Encoding:"))
        goto done;
    char *end;
    unsigned long body = strtoul(field + 18, &end, 10);
    if (end == field + 18 || strncmp(end, "\r\n", 2) || body > capacity)
        goto done;
    *length = 0;
    while (*length < body) {
        int k = mbedtls_ssl_read(&ssl, output + *length, body - *length);
        if (k <= 0)
            goto done;
        *length += (size_t)k;
    }
    result = 0;
done:
    if (fd >= 0) {
        close(fd);
    }
    mbedtls_ssl_free(&ssl);
    mbedtls_ssl_config_free(&config);
    mbedtls_x509_crt_free(&ca);
    return result;
}
#ifdef ANOVA_TLS_TEST
int main(int argc, char **argv) {
    if (argc != 5)
        return 2;
    unsigned char bytes[1024];
    size_t n = 0;
    int result = https_get(argv[1], (unsigned)strtoul(argv[2], 0, 10), argv[3], "/hello", bytes,
                           sizeof bytes, &n);
    int success = strcmp(argv[4], "accept") == 0;
    uint32_t required = !strcmp(argv[4], "reject-name")      ? MBEDTLS_X509_BADCERT_CN_MISMATCH
                        : !strcmp(argv[4], "reject-ca")      ? MBEDTLS_X509_BADCERT_NOT_TRUSTED
                        : !strcmp(argv[4], "reject-expired") ? MBEDTLS_X509_BADCERT_EXPIRED
                                                             : 0;
    if (!success && (!required || handshake_error != MBEDTLS_ERR_X509_CERT_VERIFY_FAILED ||
                     !(verification_flags & required))) {
        printf("[M10:TLS:FAIL] expected verification reason=%u observed=%u handshake=%d\n",
               required, verification_flags, handshake_error);
        return 1;
    }
    if (success ? (result != 0 || n != 7 || memcmp(bytes, "M10-TLS", 7)) : result == 0) {
        puts("[M10:TLS:FAIL]");
        return 1;
    }
    printf("[M10:TLS] %s host=%s verification=required\n", success ? "accepted" : "rejected",
           argv[1]);
    return 0;
}
#endif
