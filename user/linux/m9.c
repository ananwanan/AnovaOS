// SPDX-License-Identifier: Apache-2.0
// Exercise musl threads/signals and bounded PTY byte streams, plus optional dynamic TLS in Ring 3.
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <pthread.h>
#include <signal.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <termios.h>
#include <time.h>
#include <unistd.h>
#ifdef M9_DYNAMIC
#include <dlfcn.h>
extern int m9_shared(int);
#endif
#define CHECK(x) do { if (!(x)) { dprintf(2,"[M9:FAIL] line=%d expression=%s errno=%d\n",__LINE__,#x,errno);exit(1); } } while (0)
static _Thread_local volatile int local=7;
static pthread_mutex_t lock=PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t condition=PTHREAD_COND_INITIALIZER;
static int total,started,release_workers,shared_fd;
#ifdef M9_DYNAMIC
static const char *extra_path="/lib/libm9extra.so";
#endif
static atomic_int futex_word;
static volatile sig_atomic_t signals,interrupts,sender_pid;
static void pause_short(void) {struct timespec t={0,30000000};while (nanosleep(&t,&t)<0 && errno==EINTR) {}}
static void *worker(void *argument) {
    intptr_t id=(intptr_t)argument;
    CHECK(local==7);local=(int)id;errno=(int)id;
    CHECK(pthread_mutex_lock(&lock)==0);
    started++;CHECK(pthread_cond_broadcast(&condition)==0);
    while (!release_workers) CHECK(pthread_cond_wait(&condition,&lock)==0);
    CHECK(pthread_mutex_unlock(&lock)==0);
#ifdef M9_DYNAMIC
    CHECK(m9_shared((int)id)==41+(int)id);
    void *extra=dlopen(extra_path,RTLD_NOW);
    CHECK(extra!=NULL);
    int (*fn)(int)=(int (*)(int))dlsym(extra,"m9_shared");CHECK(fn!=NULL);
    CHECK(fn((int)id)==41+(int)id);CHECK(dlclose(extra)==0);
#endif
    for (int n=0;n<500;n++) {
        CHECK(pthread_mutex_lock(&lock)==0);total++;CHECK(pthread_mutex_unlock(&lock)==0);
        CHECK(local==id);
        if ((n%37)==0) sched_yield();
    }
    if (id==11) CHECK(close(shared_fd)==0);
    return (void *)(id+100);
}
static void handler(int signal,siginfo_t *info,void *context) {
    if (!context || !info || info->si_signo!=signal) _exit(90);
    if (signal==SIGINT) interrupts++;else {signals++;sender_pid=info->si_pid;}
    __asm__ volatile("pxor %%xmm7,%%xmm7" ::: "xmm7");
}
static void wait_ok(pid_t child) {int status;CHECK(waitpid(child,&status,0)==child);CHECK(WIFEXITED(status)&&WEXITSTATUS(status)==0);}
static void thread_tests(void) {
    pthread_t a,b;void *value;
    int original=open(getenv("M9_FILE")?getenv("M9_FILE"):"/etc/motd",O_RDONLY);CHECK(original>=0);
    // Reserve a high user slot so QEMU-user's own low-fd reuse cannot mask a shared close.
    shared_fd=fcntl(original,F_DUPFD_CLOEXEC,20);CHECK(shared_fd>=20);CHECK(close(original)==0);
    CHECK(pthread_create(&a,NULL,worker,(void *)11)==0);
    CHECK(pthread_create(&b,NULL,worker,(void *)22)==0);
    CHECK(pthread_mutex_lock(&lock)==0);
    while (started!=2) CHECK(pthread_cond_wait(&condition,&lock)==0);
#ifdef M9_DYNAMIC
    // Load a new TLS module while both worker threads already exist.
    void *extra=dlopen(extra_path,RTLD_NOW);CHECK(extra!=NULL);
#endif
    release_workers=1;CHECK(pthread_cond_broadcast(&condition)==0);
    CHECK(pthread_mutex_unlock(&lock)==0);
    CHECK(pthread_join(a,&value)==0 && value==(void *)111);
    CHECK(pthread_join(b,&value)==0 && value==(void *)122);
    CHECK(total==1000 && local==7);
    CHECK(fcntl(shared_fd,F_GETFD)==-1&&errno==EBADF);
    struct timespec timeout={0,20000000};
    CHECK(syscall(SYS_futex,&futex_word,128,1,NULL)==-1 && errno==EAGAIN);
    CHECK(syscall(SYS_futex,&futex_word,128,0,&timeout)==-1 && errno==ETIMEDOUT);
    CHECK(syscall(SYS_futex,(char *)&futex_word+1,128,0,NULL)==-1 && errno==EINVAL);
    CHECK(syscall(SYS_clone,0x10000,0,0,0,0)==-1&&errno==EINVAL);
    puts("[M9:THREADS] pthread=true TLS=true mutex=true cond=true join=true shared_fd=true futex_timeout=true");
}
static void shared_mapping_tests(void) {
    atomic_int *shared=mmap(NULL,4096,PROT_READ|PROT_WRITE,MAP_SHARED|MAP_ANONYMOUS,-1,0);
    CHECK(shared!=MAP_FAILED);atomic_store(shared,0);
    pid_t child=fork();CHECK(child>=0);
    if (!child) {pause_short();atomic_store(shared,1);CHECK(syscall(SYS_futex,shared,1,1)>=0);_exit(0);}
    long result=syscall(SYS_futex,shared,0,0,NULL);
    CHECK(result==0 || (result==-1&&errno==EAGAIN));CHECK(atomic_load(shared)==1);wait_ok(child);
    CHECK(munmap(shared,4096)==0);
    puts("[M9:SHARED] fork_mapping=true futex_cross_process=true");
}
static void signal_tests(void) {
    struct sigaction action={0};action.sa_sigaction=handler;action.sa_flags=SA_SIGINFO;
    sigemptyset(&action.sa_mask);CHECK(sigaction(SIGUSR1,&action,NULL)==0);
    int fd[2];CHECK(pipe(fd)==0);
    pid_t child=fork();CHECK(child>=0);
    if (!child) {pause_short();CHECK(kill(getppid(),SIGUSR1)==0);_exit(0);}
    uint64_t expected[2]={0x1122334455667788ULL,0x99aabbccddeeff00ULL},actual[2];
    __asm__ volatile("movdqu %0,%%xmm7" :: "m"(expected) : "xmm7");
    char byte;
    CHECK(read(fd[0],&byte,1)==-1&&errno==EINTR);
    __asm__ volatile("movdqu %%xmm7,%0" : "=m"(actual));
    CHECK(memcmp(expected,actual,sizeof actual)==0);CHECK(signals==1&&sender_pid==child);wait_ok(child);
    action.sa_flags|=SA_RESTART;CHECK(sigaction(SIGUSR1,&action,NULL)==0);
    child=fork();CHECK(child>=0);
    if (!child) {pause_short();CHECK(kill(getppid(),SIGUSR1)==0);pause_short();CHECK(write(fd[1],"R",1)==1);_exit(0);}
    CHECK(read(fd[0],&byte,1)==1 && byte=='R');CHECK(signals==2);wait_ok(child);
    action.sa_flags=SA_SIGINFO;CHECK(sigaction(SIGUSR1,&action,NULL)==0);
    sigset_t mask,old;sigemptyset(&mask);sigaddset(&mask,SIGUSR1);
    CHECK(sigprocmask(SIG_BLOCK,&mask,&old)==0);CHECK(raise(SIGUSR1)==0);CHECK(signals==2);
    CHECK(ppoll(NULL,0,NULL,&old)==-1&&errno==EINTR);CHECK(signals==3&&sender_pid==getpid());
    sigset_t now;CHECK(sigprocmask(SIG_SETMASK,NULL,&now)==0);CHECK(sigismember(&now,SIGUSR1)==1);
    CHECK(sigprocmask(SIG_SETMASK,&old,NULL)==0);
    child=fork();CHECK(child>=0);
    if (!child) {pause_short();CHECK(kill(getppid(),SIGUSR1)==0);_exit(0);}
    struct timespec requested={1,0},remaining;
    CHECK(nanosleep(&requested,&remaining)==-1&&errno==EINTR);CHECK(remaining.tv_sec>0||remaining.tv_nsec>0);wait_ok(child);
    close(fd[0]);close(fd[1]);
    puts("[M9:SIGNALS] frame=true sigreturn=true SSE=true EINTR=true SA_RESTART=true ppoll_mask=true");
}
static void poll_tests(void) {
    int fd[2];CHECK(pipe(fd)==0);CHECK(fcntl(fd[0],F_SETFL,O_NONBLOCK)==0);
    char b;CHECK(read(fd[0],&b,1)==-1&&errno==EAGAIN);
    struct pollfd p={fd[0],POLLIN,0};CHECK(poll(&p,1,20)==0);
    pid_t child=fork();CHECK(child>=0);
    if (!child) {pause_short();CHECK(write(fd[1],"P",1)==1);_exit(0);}
    CHECK(poll(&p,1,1000)==1&&(p.revents&POLLIN));CHECK(read(fd[0],&b,1)==1&&b=='P');wait_ok(child);
    close(fd[1]);CHECK(poll(&p,1,0)==1&&(p.revents&POLLHUP));CHECK(read(fd[0],&b,1)==0);
    close(fd[0]);CHECK(poll(&p,1,0)==1&&(p.revents&POLLNVAL));
    puts("[M9:POLL] nonblocking=true readiness=true timeout=true EOF=true HUP=true NVAL=true");
}
/* PTYs are byte streams: a ready read may return one echo fragment, not the whole line. */
static void tty_read_exact(int fd,char *bytes,size_t length) {
    size_t done=0;
    while (done<length) {
        struct pollfd p={fd,POLLIN,0};CHECK(poll(&p,1,1000)==1&&(p.revents&POLLIN));
        ssize_t count=read(fd,bytes+done,length-done);CHECK(count>0);done+=(size_t)count;
    }
}
static void tty_tests(void) {
    int master=posix_openpt(O_RDWR|O_NOCTTY);CHECK(master>=0);
    CHECK(grantpt(master)==0);char *path=ptsname(master);CHECK(path!=NULL);
    CHECK(open(path,O_RDWR|O_NOCTTY)==-1&&errno==EIO);CHECK(unlockpt(master)==0);
    int slave=open(path,O_RDWR);CHECK(slave>=0);
    struct termios settings;CHECK(tcgetattr(slave,&settings)==0);
    CHECK((settings.c_lflag&(ICANON|ECHO))==(ICANON|ECHO));
    /* Select plain LF echo explicitly, then consume every byte before disabling echo. */
    settings.c_oflag&=~OPOST;CHECK(tcsetattr(slave,TCSANOW,&settings)==0);
    CHECK(write(master,"M9-pty\n",7)==7);char bytes[32];tty_read_exact(slave,bytes,7);CHECK(!memcmp(bytes,"M9-pty\n",7));
    tty_read_exact(master,bytes,7);CHECK(!memcmp(bytes,"M9-pty\n",7));
    cfmakeraw(&settings);CHECK(tcsetattr(slave,TCSANOW,&settings)==0);
    CHECK(write(master,"raw",3)==3);tty_read_exact(slave,bytes,3);CHECK(!memcmp(bytes,"raw",3));
    CHECK(write(slave,"reply",5)==5);
    tty_read_exact(master,bytes,5);CHECK(!memcmp(bytes,"reply",5));
    CHECK(tcsetpgrp(slave,getpgrp())==0);CHECK(tcgetpgrp(slave)==getpgrp());
    struct sigaction action={0};action.sa_sigaction=handler;action.sa_flags=SA_SIGINFO;sigemptyset(&action.sa_mask);CHECK(sigaction(SIGINT,&action,NULL)==0);
    settings.c_lflag|=ISIG;CHECK(tcsetattr(slave,TCSANOW,&settings)==0);
    /* Host line disciplines may deliver Ctrl+C after write returns. Unmask atomically while waiting. */
    sigset_t mask,old;sigemptyset(&mask);sigaddset(&mask,SIGINT);CHECK(sigprocmask(SIG_BLOCK,&mask,&old)==0);
    CHECK(write(master,"\003",1)==1);struct timespec timeout={1,0};
    CHECK(ppoll(NULL,0,&timeout,&old)==-1&&errno==EINTR);CHECK(interrupts==1);
    CHECK(sigprocmask(SIG_SETMASK,&old,NULL)==0);
    CHECK(signal(SIGHUP,SIG_IGN)!=SIG_ERR);
    close(slave);CHECK(read(master,bytes,1)==-1&&errno==EIO);close(master);
    puts("[M9:TTY] PTY=true lock=true canonical=true raw=true poll=true foreground=true CtrlC=true hangup=true");
}
static const char *self_path;
static char leader_text[32];
static void *exec_worker(void *unused) {
    (void)unused;
    execl(self_path,self_path,"exec-child",leader_text,(char *)NULL);
    _exit(91);
}
static void *fault_worker(void *unused) {(void)unused;__asm__ volatile("ud2");return NULL;}
static void *exit_worker(void *unused) {(void)unused;_exit(23);}
static void lifecycle_tests(void) {
    const char *missing=getenv("M9_BAD_INTERP");if (!missing) missing="/bin/linux-bad-interpreter";
    execl(missing,missing,(char *)NULL);CHECK(errno==ENOENT);
    pid_t child=fork();CHECK(child>=0);
    if (!child) {
        snprintf(leader_text,sizeof leader_text,"%ld",(long)getpid());
        pthread_t thread;CHECK(pthread_create(&thread,NULL,exec_worker,NULL)==0);
        for (;;) sched_yield();
    }
    int status;CHECK(waitpid(child,&status,0)==child);CHECK(WIFEXITED(status)&&WEXITSTATUS(status)==17);
    for (int operation=0;operation<3;operation++) {
        if (operation==2&&getenv("M9_QEMU_USER")) continue;
        child=fork();CHECK(child>=0);
        if (!child) {
            if (operation==2) {syscall(SYS_rt_sigreturn);_exit(92);}
            pthread_t thread;CHECK(pthread_create(&thread,NULL,operation==0?fault_worker:exit_worker,NULL)==0);
            for (;;) sched_yield();
        }
        CHECK(waitpid(child,&status,0)==child);
        if (operation==0) {CHECK(WIFSIGNALED(status)&&WTERMSIG(status)==SIGILL);}
        else if (operation==1) {CHECK(WIFEXITED(status)&&WEXITSTATUS(status)==23);}
        else {CHECK(WIFSIGNALED(status)&&WTERMSIG(status)==SIGSEGV);}
    }
    child=fork();CHECK(child>=0);
    if (!child) {CHECK(raise(SIGSTOP)==0);_exit(0);}
    pause_short();CHECK(kill(child,SIGCONT)==0);wait_ok(child);
    if (getenv("M9_QEMU_USER")) puts("[M9:HOST] exec_thread=true identity=true missing_interpreter=true fault_group=true exit_group=true bad_sigreturn=unavailable_qemu_user");
    else puts("[M9:LIFECYCLE] exec_thread=true identity=true missing_interpreter=true fault_group=true exit_group=true bad_sigreturn=true");
}
int main(int argc,char **argv) {
    setvbuf(stdout,NULL,_IOLBF,0);
    if (argc==3&&!strcmp(argv[1],"exec-child")) {
        CHECK(getpid()==strtol(argv[2],NULL,10));CHECK(syscall(SYS_gettid)==getpid());
        struct sigaction action;CHECK(sigaction(SIGUSR1,NULL,&action)==0&&action.sa_handler==SIG_DFL);return 17;
    }
    if (argc==2&&!strcmp(argv[1],"interrupt")) {
        struct sigaction action={0};action.sa_sigaction=handler;action.sa_flags=SA_SIGINFO;sigemptyset(&action.sa_mask);CHECK(sigaction(SIGINT,&action,NULL)==0);
        puts("[M9:UART] waiting");char byte;
        CHECK(read(0,&byte,1)==-1&&errno==EINTR);CHECK(interrupts==1);puts("[M9:UART] CtrlC=true EINTR=true");return 0;
    }
    self_path=getenv("M9_EXEC")?getenv("M9_EXEC"):argv[0];
    puts("[M9:START]");
#ifdef M9_DYNAMIC
    if (getenv("M9_EXTRA")) extra_path=getenv("M9_EXTRA");
    CHECK(m9_shared(3)==44);
    CHECK(dlopen("/lib/missing-m9.so",RTLD_NOW)==NULL);
    const char *error=dlerror();CHECK(error&&strstr(error,"missing-m9.so"));
    printf("[M9:MISSING-LIB] %s\n",error);
#endif
    thread_tests();shared_mapping_tests();signal_tests();poll_tests();lifecycle_tests();
    pid_t tty_child=fork();CHECK(tty_child>=0);
    if (!tty_child) {CHECK(setsid()==getpid());tty_tests();fflush(NULL);_exit(0);}wait_ok(tty_child);
    void *memory=mmap(NULL,8192,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);CHECK(memory!=MAP_FAILED);
    CHECK(mprotect((char *)memory+4096,4096,PROT_READ|PROT_WRITE)==0);*((char *)memory+4096)=42;
    int wx=mprotect(memory,4096,PROT_WRITE|PROT_EXEC);
    if (getenv("M9_HOST")) {CHECK(wx==0||(wx==-1&&errno==EACCES));} else {CHECK(wx==-1&&errno==EACCES);}CHECK(munmap(memory,8192)==0);
#ifdef M9_DYNAMIC
    puts("[M9:DYNAMIC] interpreter=true shared_library=true dlopen=true dynamic_TLS=true missing_library=true");
#else
    puts("[M9:STATIC] pthread_signals_terminal=true");
#endif
    return 0;
}
