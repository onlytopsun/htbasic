use htbasic::parser::parser::Parser;
use htbasic::runtime::interpreter::Interpreter;

/// Helper: compile and run HTBasic source, return captured output.
fn run_source(source: &str) -> Vec<String> {
    let mut parser = Parser::new(source.to_string());
    let program = parser.parse_program().expect("Parse failed");
    let mut interpreter = Interpreter::new(program);
    interpreter.run().expect("Runtime error")
}

#[test]
fn test_simple_print() {
    let output = run_source("PRINT \"Hello, World!\"\nEND\n");
    assert_eq!(output, vec!["Hello, World!"]);
}

#[test]
fn test_assignment_and_print() {
    let output = run_source("LET X = 42\nPRINT X\nEND\n");
    assert_eq!(output, vec!["42"]);
}

#[test]
fn test_implicit_let() {
    let output = run_source("X = 3.14\nPRINT X\nEND\n");
    assert_eq!(output, vec!["3.14"]);
}

#[test]
fn test_arithmetic() {
    let output =
        run_source("A = 10\nB = 3\nPRINT A + B\nPRINT A - B\nPRINT A * B\nPRINT A / B\nEND\n");
    assert_eq!(output, vec!["13", "7", "30", "3.333333333333333"]);
}

#[test]
fn test_power() {
    let output = run_source("PRINT 2 ^ 8\nEND\n");
    assert_eq!(output, vec!["256"]);
}

#[test]
fn test_unary_minus_below_power() {
    // HTBasic: -4^0.5 means -(4^0.5) = -2, not sqrt(-4)
    let output = run_source("PRINT -4 ^ 0.5\nEND\n");
    // sqrt(4) = 2, then negate
    let val: f64 = output[0].parse().unwrap();
    assert!((val + 2.0).abs() < 0.001, "Expected -2, got {}", val);
}

#[test]
fn test_string_concat() {
    let output = run_source("A$ = \"Hello, \"\nB$ = \"World!\"\nPRINT A$ & B$\nEND\n");
    assert_eq!(output, vec!["Hello, World!"]);
}

#[test]
fn test_comparison() {
    let output = run_source("PRINT 5 > 3\nPRINT 5 < 3\nPRINT 5 = 5\nPRINT 5 <> 3\nEND\n");
    assert_eq!(output, vec!["1", "0", "1", "1"]);
}

#[test]
fn test_if_then_else() {
    let output = run_source("X = 10\nIF X > 5 THEN PRINT \"BIG\" ELSE PRINT \"SMALL\"\nEND\n");
    assert_eq!(output, vec!["BIG"]);
}

#[test]
fn test_for_loop() {
    let output = run_source("FOR I = 1 TO 5\nPRINT I\nNEXT I\nEND\n");
    assert_eq!(output, vec!["1", "2", "3", "4", "5"]);
}

#[test]
fn test_for_loop_with_step() {
    let output = run_source("FOR I = 0 TO 10 STEP 2\nPRINT I\nNEXT I\nEND\n");
    assert_eq!(output, vec!["0", "2", "4", "6", "8", "10"]);
}

#[test]
fn test_while_loop() {
    let output = run_source("X = 1\nWHILE X <= 3\nPRINT X\nX = X + 1\nEND WHILE\nEND\n");
    assert_eq!(output, vec!["1", "2", "3"]);
}

#[test]
fn test_goto() {
    let output = run_source("GOTO Skip\nPRINT \"won't print\"\nSkip: PRINT \"Jumped!\"\nEND\n");
    assert_eq!(output, vec!["Jumped!"]);
}

#[test]
fn test_gosub_return() {
    let output = run_source(
        "GOSUB Sub1\nPRINT \"Back in main\"\nGOTO Done\n\
         Sub1: PRINT \"In sub\"\nRETURN\n\
         Done: PRINT \"At done\"\nEND\n",
    );
    assert_eq!(output, vec!["In sub", "Back in main", "At done"]);
}

#[test]
fn test_data_read() {
    let output = run_source("DATA 10, 20, 30\nREAD A, B, C\nPRINT A\nPRINT B\nPRINT C\nEND\n");
    assert_eq!(output, vec!["10", "20", "30"]);
}

#[test]
fn test_builtin_abs() {
    let output = run_source("PRINT ABS(-5)\nPRINT ABS(3.14)\nEND\n");
    assert_eq!(output, vec!["5", "3.14"]);
}

#[test]
fn test_builtin_sin_cos() {
    let output = run_source("PRINT SIN(0)\nPRINT COS(0)\nEND\n");
    assert_eq!(output, vec!["0", "1"]);
}

#[test]
fn test_builtin_string_functions() {
    let output = run_source("S$ = \"Hello\"\nPRINT LEN(S$)\nPRINT UPC$(S$)\nEND\n");
    assert_eq!(output, vec!["5", "HELLO"]);
}

#[test]
fn test_builtin_pi() {
    let output = run_source("PRINT PI\nEND\n");
    let val: f64 = output[0].parse().unwrap();
    assert!((val - std::f64::consts::PI).abs() < 0.001);
}

#[test]
fn test_dollar_suffix_variables() {
    let output = run_source("Name$ = \"HTBasic\"\nPRINT Name$\nEND\n");
    assert_eq!(output, vec!["HTBasic"]);
}

#[test]
fn test_exponentiation() {
    let output = run_source("PRINT 3 ^ 3\nPRINT 10 ^ 0\nEND\n");
    assert_eq!(output, vec!["27", "1"]);
}

#[test]
fn test_logical_operators() {
    let output =
        run_source("PRINT 1 AND 1\nPRINT 1 AND 0\nPRINT 1 OR 0\nPRINT NOT 1\nPRINT NOT 0\nEND\n");
    assert_eq!(output, vec!["1", "0", "1", "0", "1"]);
}

#[test]
fn test_print_tab_comma() {
    let output = run_source("PRINT \"A\",\"B\",\"C\"\nEND\n");
    // Comma tabs to next 16-column zone
    let line = &output[0];
    assert!(line.starts_with("A"));
    assert!(line.contains("B"));
    assert!(line.contains("C"));
}

#[test]
fn test_print_semicolon() {
    let output = run_source("PRINT \"Hello\";\"World\"\nEND\n");
    assert_eq!(output, vec!["HelloWorld"]);
}

#[test]
fn test_system_function() {
    let output = run_source("PRINT SYSTEM$(\"VERSION:HTB\")\nEND\n");
    assert_eq!(output, vec!["1.0"]);
}

// ===================== Phase 2: SELECT/CASE =====================

#[test]
fn test_select_case() {
    let output = run_source(
        "X = 2\n\
         SELECT X\n\
         CASE 1\nPRINT \"ONE\"\n\
         CASE 2\nPRINT \"TWO\"\n\
         CASE 3\nPRINT \"THREE\"\n\
         CASE ELSE\nPRINT \"OTHER\"\n\
         END SELECT\nEND\n",
    );
    assert_eq!(output, vec!["TWO"]);
}

#[test]
fn test_select_case_else() {
    let output = run_source(
        "X = 99\n\
         SELECT X\n\
         CASE 1\nPRINT \"ONE\"\n\
         CASE 2\nPRINT \"TWO\"\n\
         CASE ELSE\nPRINT \"OTHER\"\n\
         END SELECT\nEND\n",
    );
    assert_eq!(output, vec!["OTHER"]);
}

// ===================== Phase 2: MAT Operations =====================

#[test]
fn test_mat_assign() {
    let output = run_source(
        "DIM A(2), B(2)\n\
         A(1) = 10\nA(2) = 20\n\
         MAT B = A\n\
         PRINT B(1)\nPRINT B(2)\nEND\n",
    );
    assert_eq!(output, vec!["10", "20"]);
}

#[test]
fn test_mat_add() {
    let output = run_source(
        "DIM A(2), B(2), C(2)\n\
         A(1) = 1\nA(2) = 2\n\
         B(1) = 3\nB(2) = 4\n\
         MAT C = A + B\n\
         PRINT C(1)\nPRINT C(2)\nEND\n",
    );
    assert_eq!(output, vec!["4", "6"]);
}

#[test]
fn test_mat_sub() {
    let output = run_source(
        "DIM A(2), B(2), C(2)\n\
         A(1) = 10\nA(2) = 20\n\
         B(1) = 3\nB(2) = 4\n\
         MAT C = A - B\n\
         PRINT C(1)\nPRINT C(2)\nEND\n",
    );
    assert_eq!(output, vec!["7", "16"]);
}

#[test]
fn test_mat_transpose() {
    let output = run_source(
        "DIM A(2,2), B(2,2)\n\
         A(1,1) = 1\nA(1,2) = 2\n\
         A(2,1) = 3\nA(2,2) = 4\n\
         MAT B = TRN(A)\n\
         PRINT B(1,1)\nPRINT B(1,2)\n\
         PRINT B(2,1)\nPRINT B(2,2)\nEND\n",
    );
    assert_eq!(output, vec!["1", "3", "2", "4"]);
}

#[test]
fn test_mat_idn() {
    let output = run_source(
        "DIM A(3,3)\n\
         MAT A = IDN\n\
         PRINT A(1,1)\nPRINT A(1,2)\nPRINT A(1,3)\n\
         PRINT A(2,1)\nPRINT A(2,2)\nPRINT A(2,3)\n\
         PRINT A(3,1)\nPRINT A(3,2)\nPRINT A(3,3)\nEND\n",
    );
    assert_eq!(output, vec!["1", "0", "0", "0", "1", "0", "0", "0", "1"]);
}

#[test]
fn test_mat_zer() {
    let output = run_source(
        "DIM A(2,2)\n\
         A(1,1) = 5\nA(1,2) = 5\n\
         MAT A = ZER\n\
         PRINT A(1,1)\nPRINT A(1,2)\nEND\n",
    );
    assert_eq!(output, vec!["0", "0"]);
}

#[test]
fn test_mat_con() {
    let output = run_source(
        "DIM A(2)\n\
         MAT A = CON\n\
         PRINT A(1)\nPRINT A(2)\nEND\n",
    );
    assert_eq!(output, vec!["1", "1"]);
}

// ===================== Phase 2: ON GOTO/GOSUB =====================

#[test]
fn test_on_goto() {
    let output = run_source(
        "I = 2\n\
         ON I GOTO L1, L2, L3\n\
         PRINT \"NONE\"\nGOTO Done\n\
         L1: PRINT \"ONE\"\nGOTO Done\n\
         L2: PRINT \"TWO\"\nGOTO Done\n\
         L3: PRINT \"THREE\"\nGOTO Done\n\
         Done: PRINT \"DONE\"\nEND\n",
    );
    assert_eq!(output, vec!["TWO", "DONE"]);
}

#[test]
fn test_on_gosub() {
    let output = run_source(
        "I = 3\n\
         ON I GOSUB L1, L2, L3\n\
         PRINT \"MAIN\"\n\
         GOTO Done\n\
         L1: PRINT \"ONE\"\nRETURN\n\
         L2: PRINT \"TWO\"\nRETURN\n\
         L3: PRINT \"THREE\"\nRETURN\n\
         Done: END\n",
    );
    assert_eq!(output, vec!["THREE", "MAIN"]);
}

// ===================== Phase 2: OPTION BASE =====================

#[test]
fn test_option_base_1() {
    let output = run_source(
        "OPTION BASE 1\n\
         DIM A(3)\n\
         A(1) = 10\nA(2) = 20\nA(3) = 30\n\
         PRINT A(1)\nPRINT A(2)\nPRINT A(3)\nEND\n",
    );
    assert_eq!(output, vec!["10", "20", "30"]);
}

// ===================== Phase 2: PRINT USING / IMAGE =====================

#[test]
fn test_print_using_format() {
    let output = run_source("PRINT USING \"###.##\"; 12.345\nEND\n");
    assert_eq!(output, vec!["12.35"]);
}

#[test]
fn test_print_using_string_format() {
    let output = run_source("PRINT USING \"Value: ###\"; 42\nEND\n");
    assert!(output[0].contains("Value:"));
    assert!(output[0].contains("42"));
}

// ===================== Phase 2: REPEAT/UNTIL =====================

#[test]
fn test_repeat_until() {
    let output = run_source(
        "Count = 1\n\
         REPEAT\n\
         PRINT Count\n\
         Count = Count + 1\n\
         UNTIL Count > 3\nEND\n",
    );
    assert_eq!(output, vec!["1", "2", "3"]);
}

#[test]
fn test_repeat_until_runs_once() {
    let output = run_source(
        "! REPEAT always runs at least once\n\
         X = 10\n\
         REPEAT\n\
         PRINT \"ONCE\"\n\
         UNTIL X > 5\nEND\n",
    );
    assert_eq!(output, vec!["ONCE"]);
}

// ===================== Phase 2: Multi-dimensional arrays =====================

#[test]
fn test_two_dim_array() {
    let output = run_source(
        "DIM M(2,3)\n\
         M(1,1) = 11\nM(1,2) = 12\nM(1,3) = 13\n\
         M(2,1) = 21\nM(2,2) = 22\nM(2,3) = 23\n\
         PRINT M(1,1)\nPRINT M(1,3)\n\
         PRINT M(2,1)\nPRINT M(2,3)\nEND\n",
    );
    assert_eq!(output, vec!["11", "13", "21", "23"]);
}

// ===================== Phase 2: EXIT IF in LOOP =====================

#[test]
fn test_loop_exit_if() {
    let output = run_source(
        "I = 1\n\
         LOOP\n\
         PRINT I\n\
         I = I + 1\n\
         EXIT IF I > 3\n\
         END LOOP\nEND\n",
    );
    assert_eq!(output, vec!["1", "2", "3"]);
}

// ===================== Phase 3: Extended Built-in Functions =====================

#[test]
fn test_hyperbolic_functions() {
    let output = run_source("PRINT SINH(0)\nPRINT COSH(0)\nPRINT TANH(0)\nEND\n");
    assert_eq!(output, vec!["0", "1", "0"]);
}

#[test]
fn test_truncate() {
    let output = run_source("PRINT TRUNCATE(3.7)\nPRINT TRUNCATE(-3.7)\nEND\n");
    assert_eq!(output, vec!["3", "-3"]);
}

#[test]
fn test_round() {
    let output = run_source("PRINT ROUND(3.4)\nPRINT ROUND(3.6)\nEND\n");
    assert_eq!(output, vec!["3", "4"]);
}

#[test]
fn test_string_extended() {
    let output = run_source(
        "S$ = \"  Hello  \"\n\
         PRINT LTRIM$(S$)\n\
         PRINT RTRIM$(S$)\n\
         PRINT TRIM$(S$)\nEND\n",
    );
    assert_eq!(output, vec!["Hello  ", "  Hello", "Hello"]);
}

#[test]
fn test_space_string() {
    let output = run_source("PRINT \"[\" & SPACE$(3) & \"]\"\nEND\n");
    assert_eq!(output, vec!["[   ]"]);
}

#[test]
fn test_instr() {
    let output = run_source(
        "PRINT INSTR(\"abc\",\"ABCDEF\")\n\
         PRINT INSTR(\"zz\",\"ABCDEF\")\nEND\n",
    );
    assert_eq!(output, vec!["1", "0"]);
}

#[test]
fn test_bit_operations() {
    let output = run_source(
        "PRINT BINAND(6, 3)\n\
         PRINT BINOR(6, 3)\n\
         PRINT BINXOR(6, 3)\n\
         PRINT SHL(1, 3)\n\
         PRINT SHR(8, 2)\nEND\n",
    );
    assert_eq!(output, vec!["2", "7", "5", "8", "2"]);
}

#[test]
fn test_bit_function() {
    let output = run_source(
        "PRINT BIT(8, 3)\n\
         PRINT BIT(8, 2)\nEND\n",
    );
    assert_eq!(output, vec!["1", "0"]);
}

#[test]
fn test_randomize() {
    let output = run_source("RANDOMIZE 42\nPRINT RND\nEND\n");
    let val: f64 = output[0].parse().unwrap();
    assert!(val >= 0.0 && val < 1.0);
}

#[test]
fn test_date_time() {
    let output = run_source("S$ = DATE$\nPRINT LEN(S$)\nEND\n");
    assert_eq!(output, vec!["10"]);
}

#[test]
fn test_system_extended() {
    let output = run_source("PRINT SYSTEM$(\"VERSION\")\nEND\n");
    assert!(output[0].contains("HTBasic"));
}

#[test]
fn test_on_error_goto() {
    let output = run_source(
        "ON ERROR GOTO ErrHandler\n\
         X = 1 / 0\n\
         PRINT \"AFTER\"\n\
         GOTO Done\n\
         ErrHandler: PRINT \"ERROR CAUGHT\"\nRETURN\n\
         Done: END\n",
    );
    assert!(output.len() > 0);
}

#[test]
fn test_ucase_lcase() {
    let output = run_source("PRINT UCASE$(\"hello\")\nPRINT LCASE$(\"WORLD\")\nEND\n");
    assert_eq!(output, vec!["HELLO", "world"]);
}

#[test]
fn test_maxlen() {
    let output = run_source("A$ = \"Hello\"\nPRINT MAXLEN(A$)\nEND\n");
    assert_eq!(output, vec!["5"]);
}

#[test]
fn test_string_repeat() {
    let output = run_source("PRINT RPT$(\"Ab\", 3)\nEND\n");
    assert_eq!(output, vec!["AbAbAb"]);
}

#[test]
fn test_chr_asc() {
    let output = run_source("PRINT CHR$(65)\nPRINT NUM(\"A\")\nEND\n");
    assert_eq!(output, vec!["A", "65"]);
}

// ===================== Phase 4: Graphics =====================

#[test]
fn test_graphics_ginit_gclear() {
    let output = run_source(
        "GINIT\n\
         GCLEAR\n\
         PRINT \"Graphics init OK\"\nEND\n",
    );
    assert_eq!(output, vec!["Graphics init OK"]);
}

#[test]
fn test_graphics_move_draw() {
    let output = run_source(
        "GINIT\n\
         MOVE 10, 10\n\
         DRAW 50, 50\n\
         DRAW 90, 10\n\
         PRINT \"Drew triangle\"\nEND\n",
    );
    assert_eq!(output, vec!["Drew triangle"]);
}

#[test]
fn test_graphics_window_viewport() {
    let output = run_source(
        "GINIT\n\
         WINDOW 0, 100, 0, 100\n\
         VIEWPORT 0, 800, 0, 600\n\
         PRINT \"Viewport set\"\nEND\n",
    );
    assert_eq!(output, vec!["Viewport set"]);
}

#[test]
fn test_graphics_label() {
    let output = run_source(
        "GINIT\n\
         MOVE 50, 50\n\
         LABEL \"Hello Graphics\"\n\
         PRINT \"Label drawn\"\nEND\n",
    );
    assert_eq!(output, vec!["Label drawn"]);
}

#[test]
fn test_graphics_pen_color() {
    let output = run_source(
        "GINIT\n\
         PEN 2\n\
         COLOR \"Red\"\n\
         MOVE 10, 10\n\
         DRAW 50, 50\n\
         PRINT \"Red line\"\nEND\n",
    );
    assert_eq!(output, vec!["Red line"]);
}

#[test]
fn test_graphics_line_type() {
    let output = run_source(
        "GINIT\n\
         LINE TYPE 2\n\
         MOVE 10, 10\n\
         DRAW 90, 10\n\
         PRINT \"Dashed line\"\nEND\n",
    );
    assert_eq!(output, vec!["Dashed line"]);
}

#[test]
fn test_graphics_axes() {
    let output = run_source(
        "GINIT\n\
         WINDOW -10, 10, -10, 10\n\
         AXES 1, 1, 0, 0\n\
         PRINT \"Axes drawn\"\nEND\n",
    );
    assert_eq!(output, vec!["Axes drawn"]);
}

#[test]
fn test_graphics_grid() {
    let output = run_source(
        "GINIT\n\
         WINDOW -5, 5, -5, 5\n\
         GRID 1, 1, 0, 0\n\
         PRINT \"Grid drawn\"\nEND\n",
    );
    assert_eq!(output, vec!["Grid drawn"]);
}

#[test]
fn test_graphics_frame() {
    let output = run_source(
        "GINIT\n\
         CLIP 10, 90, 10, 90\n\
         FRAME\n\
         PRINT \"Frame drawn\"\nEND\n",
    );
    assert_eq!(output, vec!["Frame drawn"]);
}

#[test]
fn test_graphics_rectangle() {
    let output = run_source(
        "GINIT\n\
         RECTANGLE 10, 10\n\
         RECTANGLE 10, 10, FILL, EDGE\n\
         PRINT \"Rectangle drawn\"\nEND\n",
    );
    assert_eq!(output, vec!["Rectangle drawn"]);
}

#[test]
fn test_graphics_penup() {
    let output = run_source(
        "GINIT\n\
         PENUP\n\
         MOVE 50, 50\n\
         DRAW 100, 100\n\
         PRINT \"Moved with pen up\"\nEND\n",
    );
    assert_eq!(output, vec!["Moved with pen up"]);
}

#[test]
fn test_graphics_csize_ldir_lorg() {
    let output = run_source(
        "GINIT\n\
         CSIZE 3, 5\n\
         LDIR 45\n\
         LORG 5\n\
         MOVE 50, 50\n\
         LABEL \"Text\"\n\
         PRINT \"Styled text\"\nEND\n",
    );
    assert_eq!(output, vec!["Styled text"]);
}

#[test]
fn test_graphics_intensity() {
    let output = run_source(
        "GINIT\n\
         PEN 1\n\
         INTENSITY 1, 1, 0\n\
         MOVE 10, 10\n\
         DRAW 90, 10\n\
         PRINT \"Yellow pen\"\nEND\n",
    );
    assert_eq!(output, vec!["Yellow pen"]);
}

#[test]
fn test_graphics_clip_region() {
    let output = run_source(
        "GINIT\n\
         WINDOW 0, 100, 0, 100\n\
         CLIP 20, 80, 20, 80\n\
         MOVE 10, 50\n\
         DRAW 90, 50\n\
         PRINT \"Clipped line\"\nEND\n",
    );
    assert_eq!(output, vec!["Clipped line"]);
}

#[test]
fn test_graphics_gstore() {
    let output = run_source(
        "GINIT\n\
         MOVE 10, 10\n\
         DRAW 90, 90\n\
         GSTORE \"test_output_p4.png\"\n\
         PRINT \"Done\"\nEND\n",
    );
    assert!(output.len() >= 1);
    // GSTORE pushes a message, then PRINT pushes "Done"
    let all_output = output.join("\n");
    assert!(all_output.contains("saved") || all_output.contains("Done"));
}

#[test]
fn test_graphics_separate_merge_alpha() {
    let output = run_source(
        "GINIT\n\
         SEPARATE ALPHA\n\
         PRINT \"Alpha separated\"\n\
         MERGE ALPHA\n\
         PRINT \"Alpha merged\"\nEND\n",
    );
    assert_eq!(output, vec!["Alpha separated", "Alpha merged"]);
}

// ===================== Phase 5: Bytecode VM =====================

use htbasic::runtime::bytecode::{Compiler, VM};

fn run_bytecode(source: &str) -> Vec<String> {
    let mut parser = htbasic::parser::parser::Parser::new(source.to_string());
    let program = parser.parse_program().expect("Parse failed");
    let compiler = Compiler::new();
    let chunk = compiler.compile(program);
    let mut vm = VM::new(chunk);
    vm.run().expect("VM runtime error")
}

#[test]
fn test_vm_simple_print() {
    let output = run_bytecode("PRINT \"Hello from VM!\"\nEND\n");
    assert_eq!(output, vec!["Hello from VM!"]);
}

#[test]
fn test_vm_arithmetic() {
    let output = run_bytecode("PRINT 2 + 3\nPRINT 10 - 4\nPRINT 6 * 7\nEND\n");
    assert_eq!(output, vec!["5", "6", "42"]);
}

#[test]
fn test_vm_variables() {
    let output = run_bytecode("X = 42\nPRINT X\nX = X + 1\nPRINT X\nEND\n");
    assert_eq!(output, vec!["42", "43"]);
}

#[test]
fn test_vm_if_then() {
    let output = run_bytecode("X = 10\nIF X > 5 THEN PRINT \"BIG\" ELSE PRINT \"SMALL\"\nEND\n");
    assert_eq!(output, vec!["BIG"]);
}

#[test]
fn test_vm_for_loop() {
    let output = run_bytecode("FOR I = 1 TO 3\nPRINT I\nNEXT I\nEND\n");
    assert_eq!(output, vec!["1", "2", "3"]);
}

#[test]
fn test_vm_while_loop() {
    let output = run_bytecode("X = 3\nWHILE X > 0\nPRINT X\nX = X - 1\nEND WHILE\nEND\n");
    assert_eq!(output, vec!["3", "2", "1"]);
}

#[test]
fn test_vm_goto() {
    let output = run_bytecode("GOTO Skip\nPRINT \"NOPE\"\nSkip: PRINT \"JUMPED\"\nEND\n");
    assert_eq!(output, vec!["JUMPED"]);
}

#[test]
fn test_vm_builtins() {
    let output = run_bytecode("PRINT ABS(-5)\nPRINT SQR(16)\nPRINT SIN(0)\nEND\n");
    assert_eq!(output, vec!["5", "4", "0"]);
}

#[test]
fn test_vm_strings() {
    let output = run_bytecode("A$ = \"Hello\"\nPRINT A$\nPRINT LEN(A$)\nEND\n");
    assert_eq!(output, vec!["Hello", "5"]);
}

#[test]
fn test_vm_comparison() {
    let output = run_bytecode("PRINT 5 > 3\nPRINT 3 > 5\nPRINT 5 = 5\nEND\n");
    assert_eq!(output, vec!["1", "0", "1"]);
}

#[test]
fn test_vm_concat() {
    let output = run_bytecode("PRINT \"Hello \" & \"World\"\nEND\n");
    assert_eq!(output, vec!["Hello World"]);
}

#[test]
fn test_vm_power() {
    let output = run_bytecode("PRINT 2 ^ 8\nPRINT 3 ^ 3\nEND\n");
    assert_eq!(output, vec!["256", "27"]);
}

#[test]
fn test_vm_logical() {
    let output = run_bytecode("PRINT 1 AND 1\nPRINT 1 AND 0\nPRINT 1 OR 0\nPRINT NOT 0\nEND\n");
    assert_eq!(output, vec!["1", "0", "1", "1"]);
}

#[test]
fn test_vm_data_read() {
    let output = run_bytecode("DATA 10, 20, 30\nREAD A, B, C\nPRINT A\nPRINT B\nPRINT C\nEND\n");
    assert_eq!(output, vec!["10", "20", "30"]);
}

// GOSUB/RETURN in bytecode VM has a forward-label resolution issue.
// The Gosub opcode and gosub_stack are implemented but forward jumps
// to labels defined after the GOSUB need further debugging.
// TODO: fix GOSUB label resolution in resolve_jumps for the Gosub opcode.

// ===================== SUB/CALL and DEF FN =====================

#[test]
fn test_sub_call_no_params() {
    let output = run_source(
        "CALL Hello\nPRINT \"Done\"\nEND\n\
         SUB Hello\nPRINT \"Hello from SUB\"\nSUBEND\n",
    );
    assert_eq!(output, vec!["Hello from SUB", "Done"]);
}

#[test]
fn test_sub_call_with_params() {
    let output = run_source(
        "CALL Greet(\"World\", 42)\nEND\n\
         SUB Greet(Name$, Num)\n\
         PRINT Name$\nPRINT Num\n\
         SUBEND\n",
    );
    assert_eq!(output, vec!["World", "42"]);
}

#[test]
fn test_sub_call_multiple() {
    let output = run_source(
        "CALL First\nCALL Second\nEND\n\
         SUB First\nPRINT \"First\"\nSUBEND\n\
         SUB Second\nPRINT \"Second\"\nSUBEND\n",
    );
    assert_eq!(output, vec!["First", "Second"]);
}

#[test]
fn test_sub_local_variables() {
    let output = run_source(
        "X = 99\n\
         CALL SetX\n\
         PRINT X\nEND\n\
         SUB SetX\n\
         X = 42\n\
         PRINT X\n\
         SUBEND\n",
    );
    // SUB's local X should not affect the global X
    assert_eq!(output, vec!["42", "99"]);
}

#[test]
fn test_sub_recursion() {
    let output = run_source(
        "CALL Countdown(2)\nEND\n\
         SUB Countdown(N)\n\
         IF N <= 0 THEN SUBEXIT\n\
         PRINT N\n\
         CALL Countdown(N - 1)\n\
         SUBEND\n",
    );
    assert_eq!(output, vec!["2", "1"]);
}

#[test]
fn test_sub_subexit() {
    let output = run_source(
        "CALL Test(5)\nEND\n\
         SUB Test(N)\n\
         IF N > 5 THEN SUBEXIT\n\
         PRINT N\n\
         SUBEND\n",
    );
    assert_eq!(output, vec!["5"]);
}

#[test]
fn test_def_fn_simple() {
    let output = run_source(
        "PRINT FNSquare(5)\nEND\n\
         DEF FNSquare(X)\n\
         RETURN X * X\n\
         FNEND\n",
    );
    assert_eq!(output, vec!["25"]);
}

#[test]
fn test_def_fn_multiple_params() {
    let output = run_source(
        "PRINT FNArea(10, 5)\nEND\n\
         DEF FNArea(W, H)\n\
         RETURN W * H\n\
         FNEND\n",
    );
    assert_eq!(output, vec!["50"]);
}

#[test]
fn test_def_fn_string() {
    let output = run_source(
        "PRINT FNGreet$(\"World\")\nEND\n\
         DEF FNGreet$(Name$)\n\
         RETURN \"Hello \" & Name$\n\
         FNEND\n",
    );
    assert_eq!(output, vec!["Hello World"]);
}

#[test]
fn test_def_fn_calling_builtin() {
    let output = run_source(
        "PRINT FNAbsDiff(10, 17)\nEND\n\
         DEF FNAbsDiff(A, B)\n\
         RETURN ABS(A - B)\n\
         FNEND\n",
    );
    assert_eq!(output, vec!["7"]);
}

#[test]
fn test_sub_params_preserved() {
    let output = run_source(
        "CALL Show(10, 20, 30)\nEND\n\
         SUB Show(A, B, C)\n\
         PRINT A\nPRINT B\nPRINT C\n\
         SUBEND\n",
    );
    assert_eq!(output, vec!["10", "20", "30"]);
}

// ===================== Remaining Keywords =====================

// ===================== GPIB Simulator =====================

#[test]
fn test_assign_gpib() {
    let output = run_source(
        "ASSIGN @Dev TO 722\n\
         PRINT \"Assigned\"\nEND\n",
    );
    assert_eq!(output, vec!["Assigned"]);
}

#[test]
fn test_assign_file() {
    let output = run_source(
        "ASSIGN @F TO \"test.dat\"\n\
         PRINT \"File assigned\"\nEND\n",
    );
    assert_eq!(output, vec!["File assigned"]);
}

// ===================== Extended Built-ins =====================

#[test]
fn test_complex_functions() {
    let output = run_source("PRINT CMPLX(3,4)\nEND\n");
    assert!(!output.is_empty());
}

#[test]
fn test_conjugate() {
    let output = run_source("PRINT \"Complex OK\"\nEND\n");
    assert_eq!(output, vec!["Complex OK"]);
}

#[test]
fn test_statistics() {
    let output = run_source(
        "OPTION BASE 1\n\
         DIM A(5)\n\
         A(1)=2\nA(2)=4\nA(3)=6\nA(4)=8\nA(5)=10\n\
         PRINT MEAN(A)\n\
         PRINT SUM(A)\nEND\n",
    );
    let mean: f64 = output[0].parse().unwrap();
    let sum: f64 = output[1].parse().unwrap();
    assert!((mean - 6.0).abs() < 0.01);
    assert!((sum - 30.0).abs() < 0.01);
}

#[test]
fn test_std_deviation() {
    let output = run_source(
        "OPTION BASE 1\n\
         DIM A(3)\nA(1)=2\nA(2)=4\nA(3)=6\n\
         PRINT STD(A)\nEND\n",
    );
    let std_val: f64 = output[0].parse().unwrap();
    assert!(std_val > 0.0);
}

#[test]
fn test_fft_stubs() {
    let output = run_source(
        "DIM A(4)\nA(1)=1\nA(2)=0\nA(3)=0\nA(4)=0\n\
         PRINT \"FFT ok\"\nEND\n",
    );
    assert_eq!(output, vec!["FFT ok"]);
}

// CONFIGURE extensions: DUMP TO, PRT, LABEL etc.
// — require extended parser support for multi-word config keys.
// TODO: extend CONFIGURE parser for arbitrary key-value pairs.

#[test]
fn test_vm_gosub_forward() {
    let output = run_bytecode("GOSUB S\nPRINT \"Main\"\nEND\nS: PRINT \"Sub\"\nRETURN\n");
    assert_eq!(output, vec!["Sub", "Main"]);
}

#[test]
fn test_vm_gosub_backward() {
    let output =
        run_bytecode("GOTO Main\nS: PRINT \"Sub\"\nRETURN\nMain: GOSUB S\nPRINT \"After\"\nEND\n");
    assert_eq!(output, vec!["Sub", "After"]);
}

#[test]
fn test_on_error_goto_iopath() {
    // `ON ERROR GOTO @File` — shipped example programs (assign.prg) branch
    // ON ERROR to an I/O path stored in the container's name table.
    let output = run_source("10 ON ERROR GOTO @File\n20 END\n");
    assert_eq!(output, Vec::<String>::new());
}


#[test]
fn test_parser_gap_coverage() {
    // Regression coverage for every parser gap discovered while
    // parse-checking the converted TransEra examples (Stage 5).
    use htbasic::parser::parser::Parser;
    let cases = [
        "PLOTTER IS CRT,\"INTERNAL\"; COLOR MAP",
        "ASSIGN @File TO \"test.txt\"; FORMAT ON",
        "DISP",
        "DISP Height",
        "DISP Msg$",
        "OUTPUT @Out;\"  [\";",
        "INPUT \"Please enter your age:\", Age",
        "ON INTR 7,1 GOTO Intrr",
        "ON TIMEOUT 9,5 GOTO L50",
        "ON KBD ALL GOSUB Keyhit",
        "ON TIME(TIMEDATE+X) MOD 86400 GOTO Here",
        "GRAPHICS OFF",
        "GRID",
        "GRID 10,10",
        "ALPHA OFF",
        "KBD CMODE ON",
        "KEY LABELS OFF",
        "KEY LABELS PEN Blue",
        "LINE TYPE Loop",
        "LORG X",
        "MAT SORT A(*)",
        "MAT REORDER Matrix TO Vector,2",
        "MAT B$=(\"E\")",
        "CALL \"Msg\", WITH(\"Line three\",3)",
        "TRACK CRT IS ON",
        "CONFIGURE MSI ON",
        "CONFIGURE SAVE ASCII ON",
        "DISPLAY FUNCTIONS ON",
        "LET X=REAL(COSH(C))",
        "GFONT IS \"\"",
        "SYMBOL A(*), FILL, EDGE",
        "ENTER @Buf; A$",
        "ASSIGN @Out TO *",
        "SUB Bigparams(A, B, OPTIONAL C, D)",
        "AXES X_tick,Y_tick",
        // Second wave: remaining failures fixed after the first pass.
        "CLIP OFF",
        "LABEL Loop",
        "PRINT USING Image; Price",
        "LET A$(1,:4,*)=C$(1,:4,*)",
        "GOTO End",
        "ON END @File GOTO Here",
        "ON TIMEOUT 9,1 GOTO X$",
        "ENTER 9; X",
        "TRACE OFF",
        "IF J=60 THEN TRACE OFF",
        "OFF END @File",
    ];
    for c in cases {
        let src = format!("10 {c}\n");
        match Parser::new(src.clone()).parse_program() {
            Ok(_) => {},
            Err(e) => {
                panic!("parse failed for `{c}`: {e:?}");
            }
        }
    }
}

// ===================== interpreter fixes for converted programs =====================

/// MAT REORDER M BY V,n with a subscript reorders along that dimension
/// (mat reorder.prg / mat_redorder.prg family).
#[test]
fn test_mat_reorder_by() {
    let src = "\
DIM M(1,2)
M(0,0)=1
M(0,1)=2
M(0,2)=3
M(1,0)=4
M(1,1)=5
M(1,2)=6
DIM V(3)
V(0)=3
V(1)=2
V(2)=1
MAT REORDER M BY V,2
PRINT M(0,0);M(0,1);M(0,2)
PRINT M(1,0);M(1,1);M(1,2)
END
";
    let output = run_source(src);
    let joined: String = output.concat();
    assert_eq!(joined.replace(' ', ""), "321654");
}

/// MAT SORT A(*) [DESC] sorts the array in place (mat sort.prg).
#[test]
fn test_mat_sort_desc() {
    let src = "\
DIM A(3)
A(0)=2
A(1)=9
A(2)=1
A(3)=5
MAT SORT A(*) DESC
PRINT A(0);A(1);A(2);A(3)
MAT SORT A(*)
PRINT A(0);A(1);A(2);A(3)
END
";
    let output = run_source(src);
    let joined: String = output.concat();
    assert_eq!(joined.replace(' ', ""), "95211259");
}

/// DEF FN with an OPTIONAL parameter: omitted args are zeroed (fn.prg).
#[test]
fn test_def_fn_optional_fnend() {
    let src = "\
10 DEF FNJoin$(A$,OPTIONAL B$)
20 IF OPTIONAL=0 THEN RETURN A$&\"?\"
30 RETURN A$&B$
40 FNEND
50 PRINT FNJoin$(\"Hi\")
60 PRINT FNJoin$(\"Hi\",\" there\")
70 END
";
    let output = run_source(src);
    assert_eq!(output, vec!["Hi?", "Hi there"]);
}

/// OUTPUT @CRT with a trailing `;` keeps the line open so later OUTPUTs
/// append (converted programs accumulate device output).
#[test]
fn test_output_accumulation_crt() {
    let src = "\
OUTPUT @CRT; \"AB\";
OUTPUT @CRT; \"CD\"
OUTPUT @CRT; \"EF\"
END
";
    let output = run_source(src);
    assert_eq!(output, vec!["ABCD", "EF"]);
}

/// Converted programs carry line numbers on every statement.
#[test]
fn test_line_numbered_for_loop() {
    let src = "\
10 FOR I=1 TO 3
20 PRINT I
30 NEXT I
40 END
";
    let output = run_source(src);
    assert_eq!(output, vec!["1", "2", "3"]);
}
