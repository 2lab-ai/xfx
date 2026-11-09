#include <stdio.h>
#include <time.h>
#include <mach/mach_time.h>
static double rd(clockid_t id){struct timespec t; clock_gettime(id,&t); return t.tv_sec + t.tv_nsec/1e9;}
int main(void){
  mach_timebase_info_data_t tb; mach_timebase_info(&tb);
  double mono_raw=rd(CLOCK_MONOTONIC_RAW), uptime_raw=rd(CLOCK_UPTIME_RAW), mono=rd(CLOCK_MONOTONIC);
  double cont=(double)mach_continuous_time()*tb.numer/tb.denom/1e9, abs_=(double)mach_absolute_time()*tb.numer/tb.denom/1e9;
  printf("CLOCK_MONOTONIC_RAW=%.3f\nCLOCK_MONOTONIC=%.3f\nmach_continuous_time=%.3f\nCLOCK_UPTIME_RAW=%.3f\nmach_absolute_time=%.3f\nslept_since_boot(MONOTONIC_RAW-UPTIME_RAW)=%.3f s\n",mono_raw,mono,cont,uptime_raw,abs_,mono_raw-uptime_raw);
  return 0;}
