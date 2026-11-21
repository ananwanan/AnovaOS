// SPDX-License-Identifier: Apache-2.0
// Exercise Linux startup, checked string/vector boundaries, partial I/O, TLS and process lifetime.
typedef unsigned long u64;
static long call(u64 n,u64 a,u64 b,u64 c,u64 d,u64 e,u64 f) {
    register u64 r10 __asm__("r10")=d, r8 __asm__("r8")=e, r9 __asm__("r9")=f;
    long out;
    __asm__ volatile("syscall":"=a"(out):"a"(n),"D"(a),"S"(b),"d"(c),"r"(r10),"r"(r8),"r"(r9):"rcx","r11","memory");
    return out;
}
#define C(n,a,b,c) call(n,(u64)(a),(u64)(b),(u64)(c),0,0,0)
#define CHECK(x) do { if (!(x)) { C(60,1,0,0); __builtin_unreachable(); } } while(0)
static int equal(const char *a,const char *b) { while(*a && *a==*b) { ++a; ++b; } return *a==*b; }

static unsigned char read_pattern(unsigned offset)
{
    return (unsigned char)(offset * 31 + (offset >> 8));
}

static void check_scalar_read(void)
{
    enum { PAGE = 4096, REQUEST = 9115, LIMIT = 16384, TAIL = 311, START = 13 };
    const long writable_prefix = 2 * PAGE - START;
    long mapping = call(9, 0, 5 * PAGE, 3, 0x22, (u64)-1, 0);
    CHECK(mapping > 0);
    unsigned char *buffer = (void *)mapping;
    long fd = C(2, "/tmp/scalar-read", 0x242, 0644);
    CHECK(fd >= 0);
    for (unsigned i = 0; i < LIMIT + TAIL; ++i)
        buffer[i] = read_pattern(i);
    for (unsigned offset = 0; offset < LIMIT; offset += PAGE)
        CHECK(C(1, fd, buffer + offset, PAGE) == PAGE);
    CHECK(C(1, fd, buffer + LIMIT, TAIL) == TAIL);

    CHECK(C(8, fd, 0, 0) == 0);
    CHECK(C(0, fd, buffer + START, REQUEST) == REQUEST);
    for (unsigned i = 0; i < REQUEST; ++i)
        CHECK(buffer[START + i] == read_pattern(i));
    CHECK(C(8, fd, 0, 1) == REQUEST);

    CHECK(C(8, fd, 0, 0) == 0);
    CHECK(C(0, fd, buffer, LIMIT + TAIL) == LIMIT);
    CHECK(C(0, fd, buffer, REQUEST) == TAIL);
    for (unsigned i = 0; i < TAIL; ++i)
        CHECK(buffer[i] == read_pattern(LIMIT + i));
    CHECK(C(0, fd, buffer, REQUEST) == 0);

    CHECK(C(8, fd, 0, 0) == 0);
    CHECK(C(10, buffer + 2 * PAGE, PAGE, 0) == 0);
    CHECK(C(0, fd, buffer + 2 * PAGE, REQUEST) == -14);
    CHECK(C(8, fd, 0, 1) == 0);
    CHECK(C(0, fd, buffer + START, REQUEST) == writable_prefix);
    for (unsigned i = 0; i < (unsigned)writable_prefix; ++i)
        CHECK(buffer[START + i] == read_pattern(i));
    CHECK(C(8, fd, 0, 1) == writable_prefix);
    CHECK(C(0, fd, buffer, 1) == 1 && buffer[0] == read_pattern((unsigned)writable_prefix));
    CHECK(C(3, fd, 0, 0) == 0 && C(87, "/tmp/scalar-read", 0, 0) == 0);
    CHECK(C(11, mapping, 5 * PAGE, 0) == 0);
    static const char marker[] = "[LINUX:READ] regular=9115 cap=16384 eof=true partial=8179 offset=true\n";
    CHECK(C(1, 1, marker, sizeof(marker) - 1) == sizeof(marker) - 1);
}

void probe_main(u64 *stack) {
    u64 argc=*stack;
    char **argv=(void*)(stack+1), **env=argv+argc+1;
    u64 *aux=(void*)env;
    while(*aux) ++aux;
    ++aux;
    int page=0,entry=0,phdr=0,random=0;
    for(;aux[0];aux+=2) {
        if(aux[0]==6) page=aux[1]==4096;
        if(aux[0]==9) entry=aux[1]!=0;
        if(aux[0]==3) phdr=aux[1]!=0;
        if(aux[0]==25) random=aux[1]!=0;
    }
    CHECK(page && entry && phdr && random);
    if(argc==2 && equal(argv[1],"exit-high")) { C(60,0x100,0,0); __builtin_unreachable(); }
    if(argc==2 && equal(argv[1],"after-exec")) {
        CHECK(env[0] && equal(env[0],"M8=environment") && !env[1]);
        CHECK(C(72,10,1,0)==-9);
        char byte=0;
        CHECK(C(0,3,&byte,1)==1 && byte=='Z');
        C(60,17,0,0); __builtin_unreachable();
    }
    CHECK(C(9999,0,0,0)==-38);
    CHECK(C(0,31,0,0)==-9);
    CHECK(C(1,1,1,1)==-14);
    CHECK(C(2,"/missing",0,0)==-2);
    check_scalar_read();
    long p=call(9,0,8192,3,0x22,(u64)-1,0);
    CHECK(p>0);
    volatile char *mem=(void*)p;
    CHECK(mem[0]==0 && mem[8191]==0);
    mem[8191]=42;
    CHECK(C(158,0x1002,p,0)==0);
    u64 base=0;
    CHECK(C(158,0x1003,&base,0)==0 && base==(u64)p);
    CHECK(C(158,0x1002,0x800000000000UL,0)==-22);
    long child=C(57,0,0,0);
    CHECK(child>=0);
    if(child==0) { CHECK(mem[8191]==42); mem[8191]=7; C(60,0x117,0,0); __builtin_unreachable(); }
    unsigned status=0;
    CHECK(C(61,child,&status,0)==child && status==(23U<<8));
    CHECK(mem[8191]==42);
    CHECK(C(11,p,8192,0)==0);
    long fd=C(2,"/tmp/probe",0x242,0644);
    CHECK(fd==3 && C(1,fd,"Z",1)==1 && C(8,fd,0,0)==0);
    CHECK(C(83,"./",0777,0)==-17 && C(83,".",0755,0)==-17);
    CHECK(C(83,"../",0755,0)==-17 && C(83,"//",0755,0)==-17);
    CHECK(C(83,"/tmp/mkdir-probe/",0755,0)==0);
    CHECK(C(83,"/tmp/mkdir-probe/",0755,0)==-17);
    CHECK(C(83,"/tmp/mkdir-probe/./",0755,0)==-17);
    CHECK(C(83,"/tmp/missing-probe/..",0755,0)==-2);
    CHECK(C(83,"/tmp/probe/..",0755,0)==-20);
    CHECK(C(84,"/tmp/mkdir-probe",0,0)==0);
    long q=call(9,0,4096,3,0x22,(u64)-1,0);
    CHECK(q>0);
    char *path=(void*)(q+4096-11);
    static const char guard_path[]="/tmp/guard";
    for(unsigned i=0;i<sizeof(guard_path);++i) path[i]=guard_path[i];
    long guard=C(2,path,0x242,0644);
    CHECK(guard>=0 && C(3,guard,0,0)==0 && C(87,path,0,0)==0);
    CHECK(C(59,"/bin/linux-probe",q+4096,0)==-14);
    enum { EXEC_VECTOR_LIMIT = 512 };
    const char *arguments[EXEC_VECTOR_LIMIT+2];
    for(unsigned i=0;i<EXEC_VECTOR_LIMIT;++i) arguments[i]="x";
    arguments[EXEC_VECTOR_LIMIT]=0;
    CHECK(C(59,"/missing",arguments,0)==-2);
    arguments[EXEC_VECTOR_LIMIT]="x";
    arguments[EXEC_VECTOR_LIMIT+1]=0;
    CHECK(C(59,"/bin/linux-probe",arguments,0)==-7);
    char *payload=(void*)q;
    payload[0]='V'; payload[1]='W';
    u64 *iov=(void*)(q+4096-16);
    iov[0]=(u64)payload; iov[1]=2;
    CHECK(C(8,fd,0,2)==1);
    CHECK(C(20,fd,q+4096,1)==-14);
    CHECK(C(20,fd,iov,2)==-14 && C(8,fd,0,1)==1);
    u64 invalid_length[]={q,2,q,(u64)-1};
    CHECK(C(20,fd,invalid_length,2)==-22 && C(8,fd,0,1)==1);
    u64 invalid_buffer[]={q,2,1,1};
    CHECK(C(20,fd,invalid_buffer,2)==2);
    CHECK(C(20,31,0,0)==-9);
    CHECK(C(318,0,0,0)==0);
    CHECK(C(8,fd,0,0)==0 && C(11,q,4096,0)==0);
    static const char boundary[]="[LINUX:BOUNDARY] page_end=true vector_fault=true writev_partial=true empty_random=true\n";
    CHECK(C(1,1,boundary,sizeof(boundary)-1)==sizeof(boundary)-1);
    CHECK(C(72,fd,1030,10)==10 && C(72,10,1,0)==1);
    child=C(57,0,0,0); CHECK(child>=0);
    if(child==0) {
        const char *args[]={"/bin/linux-probe","after-exec",0};
        const char *env[]={"M8=environment",0};
        CHECK(C(59,"/missing",args,env)==-2);
        C(59,"/bin/linux-probe",args,env);
        C(60,99,0,0); __builtin_unreachable();
    }
    CHECK(C(61,child,&status,0)==child && status==(17U<<8));
    CHECK(C(3,10,0,0)==0 && C(3,fd,0,0)==0 && C(87,"/tmp/probe",0,0)==0);
    u64 pipefd[1]={0};
    CHECK(C(22,pipefd,0,0)==0);
    unsigned *fds=(void*)pipefd;
    CHECK(C(3,fds[0],0,0)==0);
    child=C(57,0,0,0); CHECK(child>=0);
    if(child==0) { C(1,fds[1],"x",1); C(60,99,0,0); __builtin_unreachable(); }
    CHECK(C(61,child,&status,0)==child && status==13);
    u64 action[4]={1,0,0,0};
    CHECK(call(13,13,(u64)action,0,8,0,0)==0);
    CHECK(C(1,fds[1],"x",1)==-32);
    CHECK(C(3,fds[1],0,0)==0);
    unsigned char entropy[32];
    CHECK(C(318,entropy,32,0)==32 && C(318,1,32,0)==-14);
    u64 ts[2]={0,10000000};
    CHECK(C(35,ts,0,0)==0);
    static const char text[]="[LINUX:PROBE] errno=true mmap=true TLS=true fork=true wait=true exec=true env=true CLOEXEC=true SIGPIPE=true random=true sleep=true\n";
    CHECK(C(1,1,text,sizeof(text)-1)==sizeof(text)-1);
    C(60,0,0,0);
    __builtin_unreachable();
}
