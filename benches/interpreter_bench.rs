use criterion::{black_box, criterion_group, criterion_main, Criterion};
use htbasic::parser::parser::Parser;
use htbasic::runtime::bytecode::{Compiler, VM};
use htbasic::runtime::interpreter::Interpreter;

/// Run source through the tree-walking interpreter.
fn run_tree(source: &str) -> Vec<String> {
    let mut parser = Parser::new(source.to_string());
    let program = parser.parse_program().expect("Parse failed");
    let mut interpreter = Interpreter::new(program);
    interpreter.run().expect("Tree-walking runtime error")
}

/// Run source through the bytecode VM.
fn run_bytecode(source: &str) -> Vec<String> {
    let mut parser = Parser::new(source.to_string());
    let program = parser.parse_program().expect("Parse failed");
    let compiler = Compiler::new();
    let chunk = compiler.compile(program);
    let mut vm = VM::new(chunk);
    vm.run().expect("VM runtime error")
}

// ===================== Benchmark Programs =====================

const ARITHMETIC_LOOP: &str = "\
FOR I = 1 TO 200\n\
  X = I * 3 + I / 2\n\
  Y = X ^ 2\n\
NEXT I\n\
PRINT Y\n\
END\n";

const FIBONACCI: &str = "\
A = 0\n\
B = 1\n\
FOR I = 1 TO 25\n\
  C = A + B\n\
  A = B\n\
  B = C\n\
NEXT I\n\
PRINT B\n\
END\n";

const STRING_OPS: &str = "\
S$ = \"\"\n\
FOR I = 1 TO 100\n\
  S$ = S$ & \"X\"\n\
NEXT I\n\
PRINT LEN(S$)\n\
END\n";

const ARRAY_OPS: &str = "\
DIM A(50,50), B(50,50), C(50,50)\n\
FOR I = 1 TO 50\n\
  FOR J = 1 TO 50\n\
    A(I,J) = I + J\n\
    B(I,J) = I * J\n\
  NEXT J\n\
NEXT I\n\
MAT C = A + B\n\
PRINT C(25,25)\n\
END\n";

const NESTED_CALLS: &str = "\
X = 0\n\
GOSUB Inc\n\
PRINT X\n\
END\n\
Inc: X = X + 1\n\
IF X < 50 THEN GOSUB Inc\n\
RETURN\n";

const MIXED_OPS: &str = "\
X = 1.5\n\
S$ = \"Hello\"\n\
FOR I = 1 TO 100\n\
  X = X * 1.01 + SIN(X)\n\
  IF I MOD 10 = 0 THEN S$ = S$ & \"!\"\n\
NEXT I\n\
PRINT X\n\
PRINT LEN(S$)\n\
END\n";

// ===================== Benchmarks =====================

fn bench_arithmetic_tree(c: &mut Criterion) {
    c.bench_function("arithmetic/tree", |b| {
        b.iter(|| run_tree(black_box(ARITHMETIC_LOOP)))
    });
}

fn bench_arithmetic_vm(c: &mut Criterion) {
    c.bench_function("arithmetic/vm", |b| {
        b.iter(|| run_bytecode(black_box(ARITHMETIC_LOOP)))
    });
}

fn bench_fibonacci_tree(c: &mut Criterion) {
    c.bench_function("fibonacci/tree", |b| {
        b.iter(|| run_tree(black_box(FIBONACCI)))
    });
}

fn bench_fibonacci_vm(c: &mut Criterion) {
    c.bench_function("fibonacci/vm", |b| {
        b.iter(|| run_bytecode(black_box(FIBONACCI)))
    });
}

fn bench_string_tree(c: &mut Criterion) {
    c.bench_function("string/tree", |b| {
        b.iter(|| run_tree(black_box(STRING_OPS)))
    });
}

fn bench_string_vm(c: &mut Criterion) {
    c.bench_function("string/vm", |b| {
        b.iter(|| run_bytecode(black_box(STRING_OPS)))
    });
}

fn bench_array_tree(c: &mut Criterion) {
    c.bench_function("array/tree", |b| {
        b.iter(|| run_tree(black_box(ARRAY_OPS)))
    });
}

fn bench_array_vm(c: &mut Criterion) {
    c.bench_function("array/vm", |b| {
        b.iter(|| run_bytecode(black_box(ARRAY_OPS)))
    });
}

fn bench_nested_tree(c: &mut Criterion) {
    c.bench_function("nested/tree", |b| {
        b.iter(|| run_tree(black_box(NESTED_CALLS)))
    });
}

fn bench_nested_vm(c: &mut Criterion) {
    c.bench_function("nested/vm", |b| {
        b.iter(|| run_bytecode(black_box(NESTED_CALLS)))
    });
}

fn bench_mixed_tree(c: &mut Criterion) {
    c.bench_function("mixed/tree", |b| {
        b.iter(|| run_tree(black_box(MIXED_OPS)))
    });
}

fn bench_mixed_vm(c: &mut Criterion) {
    c.bench_function("mixed/vm", |b| {
        b.iter(|| run_bytecode(black_box(MIXED_OPS)))
    });
}

criterion_group!(
    benches,
    bench_arithmetic_tree,
    bench_arithmetic_vm,
    bench_fibonacci_tree,
    bench_fibonacci_vm,
    bench_string_tree,
    bench_string_vm,
    bench_array_tree,
    bench_array_vm,
    bench_nested_tree,
    bench_nested_vm,
    bench_mixed_tree,
    bench_mixed_vm,
);
criterion_main!(benches);
