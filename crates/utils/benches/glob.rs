// Based off fast-glob: https://github.com/oxc-project/fast-glob/blob/main/benches/bench.rs

use criterion::{Criterion, criterion_group, criterion_main};
use starbase_sandbox::{Sandbox, create_empty_sandbox};
use starbase_utils::glob;
use std::fs;
use wax::Program;

fn simple_match(c: &mut Criterion) {
    let mut group = c.benchmark_group("simple_match");

    const GLOB: &str = "some/**/n*d[k-m]e?txt";
    const PATH: &str = "some/a/bigger/path/to/the/crazy/needle.txt";

    group.bench_function("wax", |b| {
        b.iter(|| wax::Glob::new(GLOB).unwrap().is_match(PATH))
    });

    group.bench_function("wax-pre-compiled", |b| {
        let matcher = wax::Glob::new(GLOB).unwrap();
        b.iter(|| matcher.is_match(PATH))
    });

    group.finish();
}

fn brace_expansion(c: &mut Criterion) {
    let mut group = c.benchmark_group("brace_expansion");

    const GLOB: &str = "some/**/{tob,crazy}/?*.{png,txt}";
    const PATH: &str = "some/a/bigger/path/to/the/crazy/needle.txt";

    group.bench_function("wax", |b| {
        b.iter(|| wax::Glob::new(GLOB).unwrap().is_match(PATH))
    });

    group.bench_function("wax-pre-compiled", |b| {
        let matcher = wax::Glob::new(GLOB).unwrap();
        b.iter(|| matcher.is_match(PATH))
    });

    group.finish();
}

fn create_sandbox() -> Sandbox {
    let sandbox = create_empty_sandbox();

    for c in 'a'..='z' {
        let dir = sandbox.path().join(c.to_string());

        fs::create_dir_all(&dir).unwrap();

        for i in 0..=150 {
            fs::write(dir.join(i.to_string()), "").unwrap();
        }

        for c in 'A'..='Z' {
            let sub_dir = dir.join(c.to_string());

            fs::create_dir_all(&sub_dir).unwrap();

            for i in 0..=150 {
                fs::write(sub_dir.join(format!("{i}.txt")), "").unwrap();
            }
        }
    }

    sandbox
}

fn walk(c: &mut Criterion) {
    let mut group = c.benchmark_group("walk");
    let sandbox = create_sandbox();

    group.bench_function("star-all", |b| {
        b.iter(|| glob::walk(sandbox.path(), ["**/*"]))
    });

    group.bench_function("one-depth", |b| {
        b.iter(|| glob::walk(sandbox.path(), ["*"]))
    });

    group.bench_function("two-depth", |b| {
        b.iter(|| glob::walk(sandbox.path(), ["*/*"]))
    });

    group.bench_function("txt-files", |b| {
        b.iter(|| glob::walk(sandbox.path(), ["**/*.txt"]))
    });

    group.finish();
}

fn walk_fast(c: &mut Criterion) {
    let mut group = c.benchmark_group("walk_fast");
    let sandbox = create_sandbox();

    group.bench_function("star-all", |b| {
        b.iter(|| glob::walk_fast(sandbox.path(), ["**/*"]))
    });

    group.bench_function("one-depth", |b| {
        b.iter(|| glob::walk_fast(sandbox.path(), ["*"]))
    });

    group.bench_function("two-depth", |b| {
        b.iter(|| glob::walk_fast(sandbox.path(), ["*/*"]))
    });

    group.bench_function("txt-files", |b| {
        b.iter(|| glob::walk_fast(sandbox.path(), ["**/*.txt"]))
    });

    group.finish();
}

criterion_group!(benches, simple_match, brace_expansion, walk, walk_fast);
criterion_main!(benches);
