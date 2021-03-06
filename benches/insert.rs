use criterion::{black_box, criterion_group, criterion_main, Criterion};

use flume::unbounded;
use leitio::Leitio;

fn write(n: u64, _size: usize) {
    let list = Leitio::new();

    for _i in 0..n {
        list.push(&n);
    }
}

fn write_flume(n: u64, _size: usize) {
    let (sender, _recv) = unbounded();

    for _i in 0..n {
        sender.try_send(&n).unwrap();
    }
}

fn write_read(n: u64, _size: usize) {
    let list = Leitio::new();

    for _i in 0..n {
        list.push(n);
    }

    let sheild = list.get_shield();
    for _i in 0..n {
        list.pop(&sheild).unwrap();
    }
}

fn write_read_flume(n: u64, _size: usize) {
    let (sender, recv) = unbounded();

    for _i in 0..n {
        sender.send(&n).unwrap();
    }

    for _i in 0..n {
        recv.recv().unwrap();
    }
}

fn write_benchmark(c: &mut Criterion) {
    c.bench_function("Insert 20", |b| b.iter(|| write(black_box(20), 1000)));
    c.bench_function("Insert 10,000", |b| {
        b.iter(|| write(black_box(10_000), 10_000))
    });

    // Flume here
    c.bench_function("Insert 20 flume", |b| {
        b.iter(|| write_flume(black_box(20), 1024))
    });
    c.bench_function("Insert 10,000 flume", |b| {
        b.iter(|| write_flume(black_box(10_000), 10_000))
    });
    c.bench_function("Insert 1,000,000 flume", |b| {
        b.iter(|| write_flume(black_box(1_000_000), 10_000))
    });
}

fn write_read_benchmark(c: &mut Criterion) {
    c.bench_function("Write/Read 20", |b| {
        b.iter(|| write_read(black_box(20), 1000))
    });
    c.bench_function("Write/Read 1000", |b| {
        b.iter(|| write_read(black_box(1000), 1024))
    });

    // Flume checks
    c.bench_function("Write/Read 20 flume", |b| {
        b.iter(|| write_read_flume(black_box(20), 1000))
    });
    c.bench_function("Write/Read 1_000_000 flume", |b| {
        b.iter(|| write_read_flume(black_box(1_000_000), 1024))
    });
}

criterion_group!(benches, write_benchmark, write_read_benchmark);
criterion_main!(benches);
