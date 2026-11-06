//! Scratch probe for flake-mdtime2: where the time goes for the failing
//! shape at x16384 and x65536. Not part of the crate. To run it, copy it to
//! `src/markdown/tests/probe.rs`, add `mod probe;` to
//! `src/markdown/tests/mod.rs`, and run
//! `cargo test --lib -- --exact --test-threads=1 --nocapture
//! markdown::tests::probe::probe_where_time_goes`.

use std::time::Duration;

use super::super::MarkdownProcessor;
use super::thread_cpu_time;

fn rusage() -> (i64, i64, i64) {
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    let status = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) };
    assert_eq!(status, 0);
    (ru.ru_minflt as i64, ru.ru_nivcsw as i64, ru.ru_nvcsw as i64)
}

fn line(prefix: &str, unit: &str, n: usize, suffix: &str) -> String {
    let mut line = String::from(prefix);
    line.push_str(&unit.repeat(n));
    line.push_str(suffix);
    line
}

fn stats(v: &mut [Duration]) -> (Duration, Duration, Duration) {
    v.sort();
    (v[0], v[v.len() / 2], v[v.len() - 1])
}

#[test]
fn probe_where_time_goes() {
    let shapes = [
        ("https://example.com ", "*", "\n"),
        ("text ", "*", "x\n"),
    ];
    for (prefix, unit, suffix) in shapes {
        let label = format!("{prefix:?}{unit:?}..{suffix:?}");
        // 1. Steady state on one reused processor: per-push CPU, faults, csw.
        let mut processor = MarkdownProcessor::new();
        let mut out = Vec::new();
        let short = line(prefix, unit, 16384, suffix);
        let long = line(prefix, unit, 65536, suffix);
        for _ in 0..3 {
            for l in [&short, &long] {
                out.clear();
                processor.push(l, &mut out);
            }
        }
        let mut ts = Vec::new();
        let mut tl = Vec::new();
        let mut faults = [0i64; 2];
        let mut ivcsw = [0i64; 2];
        for _ in 0..30 {
            for (k, l) in [&short, &long].into_iter().enumerate() {
                out.clear();
                let (f0, c0, _) = rusage();
                let t0 = thread_cpu_time();
                processor.push(l, &mut out);
                let dt = thread_cpu_time() - t0;
                let (f1, c1, _) = rusage();
                faults[k] += f1 - f0;
                ivcsw[k] += c1 - c0;
                if k == 0 { ts.push(dt) } else { tl.push(dt) }
            }
        }
        let (smin, smed, smax) = stats(&mut ts);
        let (lmin, lmed, lmax) = stats(&mut tl);
        eprintln!(
            "probe {label} reused x16384 min {smin:?} med {smed:?} max {smax:?} | x65536 min {lmin:?} med {lmed:?} max {lmax:?} | ratio(min) {:.2} ratio(med) {:.2} | minflt short {} long {} | ivcsw short {} long {}",
            lmin.as_secs_f64() / smin.as_secs_f64(),
            lmed.as_secs_f64() / smed.as_secs_f64(),
            faults[0], faults[1], ivcsw[0], ivcsw[1]
        );
        // 2. Fresh processor per push: the first push pays the buffer growth.
        let mut fs = Vec::new();
        let mut fl = Vec::new();
        let mut ffaults = [0i64; 2];
        for _ in 0..10 {
            for (k, l) in [&short, &long].into_iter().enumerate() {
                let mut p = MarkdownProcessor::new();
                let mut o = Vec::new();
                let (f0, _, _) = rusage();
                let t0 = thread_cpu_time();
                p.push(l, &mut o);
                let dt = thread_cpu_time() - t0;
                let (f1, _, _) = rusage();
                ffaults[k] += f1 - f0;
                if k == 0 { fs.push(dt) } else { fl.push(dt) }
            }
        }
        let (a, b, _) = stats(&mut fs);
        let (c, d, _) = stats(&mut fl);
        eprintln!(
            "probe {label} fresh x16384 min {a:?} med {b:?} | x65536 min {c:?} med {d:?} | ratio(min) {:.2} | minflt/10 short {} long {}",
            c.as_secs_f64() / a.as_secs_f64(), ffaults[0], ffaults[1]
        );
        // 3. Split: accumulate the line (no newline) vs render it ("\n").
        for n in [16384usize, 65536] {
            let body = line(prefix, unit, n, suffix.trim_end_matches('\n'));
            let mut acc = Vec::new();
            let mut ren = Vec::new();
            for _ in 0..15 {
                out.clear();
                let t0 = thread_cpu_time();
                processor.push(&body, &mut out);
                let t1 = thread_cpu_time();
                processor.push("\n", &mut out);
                let t2 = thread_cpu_time();
                acc.push(t1 - t0);
                ren.push(t2 - t1);
            }
            let (am, _, _) = stats(&mut acc);
            let (rm, _, _) = stats(&mut ren);
            eprintln!("probe {label} split x{n}: accumulate min {am:?}, render min {rm:?}");
        }
        // 4. Size sweep: per-unit cost, min of 7, reused processor.
        for n in [1024usize, 4096, 16384, 65536, 262144, 1048576] {
            let l = line(prefix, unit, n, suffix);
            let mut best = Duration::MAX;
            for _ in 0..7 {
                out.clear();
                let t0 = thread_cpu_time();
                processor.push(&l, &mut out);
                best = best.min(thread_cpu_time() - t0);
            }
            eprintln!(
                "probe {label} sweep x{n}: min {best:?}, {:.2} ns/unit",
                best.as_nanos() as f64 / n as f64
            );
        }
    }
}
