// SPDX-License-Identifier: Apache-2.0
// Exercise the Linux register ABI, errno isolation, anonymous pages, TLS and fork/wait.
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
