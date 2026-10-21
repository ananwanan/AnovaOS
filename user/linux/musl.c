// SPDX-License-Identifier: Apache-2.0
// Verify an unmodified musl startup, compiler TLS, errno, allocation and real file I/O.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <fcntl.h>
#include <unistd.h>
#include <errno.h>
#include <sys/stat.h>
static __thread volatile int tls_value=73;
int main(int argc,char **argv) {
    if((argc!=2 && argc!=3) || strcmp(argv[1],"test") || tls_value!=73) return 1;
    const char *path=argc==3 ? argv[2] : "/tmp/musl.txt";
    puts("[LINUX:MUSL] hello");
    char *p=malloc(200000);
    if(!p) return 2;
    memset(p,42,200000);
    if(p[199999]!=42) return 3;
    free(p);
    errno=0;
    if(open("/missing",O_RDONLY)!=-1 || errno!=ENOENT) return 4;
    int fd=open(path,O_CREAT|O_TRUNC|O_RDWR,0644);
    if(fd<0 || write(fd,"musl-data\n",10)!=10 || lseek(fd,0,SEEK_SET)!=0) return 5;
    char bytes[16]={0};
    if(read(fd,bytes,16)!=10 || strcmp(bytes,"musl-data\n")) return 6;
    struct stat st;
    if(fstat(fd,&st) || st.st_size!=10 || close(fd) || unlink(path)) return 7;
    puts("[LINUX:MUSL] TLS=true malloc=true errno=true file=true");
    return 0;
}
