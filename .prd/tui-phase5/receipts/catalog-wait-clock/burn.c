/* One process of busy threads: `burn <threads> <seconds>` spins <threads>
 * threads for <seconds>, then exits by itself. It never forks or execs. */
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static volatile int stopping = 0;

static void *spin(void *unused) {
    volatile unsigned long count = 0;
    (void)unused;
    while (!stopping) {
        count++;
    }
    return NULL;
}

int main(int argc, char **argv) {
    if (argc != 3) {
        fprintf(stderr, "usage: burn <threads> <seconds>\n");
        return 2;
    }
    int threads = atoi(argv[1]);
    int seconds = atoi(argv[2]);
    if (threads < 1 || threads > 512 || seconds < 1 || seconds > 7200) {
        fprintf(stderr, "burn: threads 1..512, seconds 1..7200\n");
        return 2;
    }
    pthread_t *spinners = calloc((size_t)threads, sizeof(pthread_t));
    if (spinners == NULL) {
        return 1;
    }
    int started = 0;
    for (; started < threads; started++) {
        if (pthread_create(&spinners[started], NULL, spin, NULL) != 0) {
            break;
        }
    }
    struct timespec limit = {seconds, 0};
    nanosleep(&limit, NULL);
    stopping = 1;
    for (int i = 0; i < started; i++) {
        pthread_join(spinners[i], NULL);
    }
    free(spinners);
    return 0;
}
