//! Code generation: typed AST → WebAssembly core module.
//!
//! Two-pass strategy (same as kotoba-clj):
//!   Pass 1 — assign stable function indices, evaluate `def` constants.
//!   Pass 2 — emit function bodies.
//!
//! ## Value model extension for f32
//!
//! All guest values are `i64` on the WASM stack.  F32 values are stored as
//! their IEEE-754 bit-pattern zero-extended to 64 bits.  When calling a host
//! import that expects an `f32` parameter, the codegen emits:
//!   `i32.wrap_i64`        — drop the high 32 zero bits
//!   `f32.reinterpret_i32` — reinterpret the int bits as an f32
//!
//! When a host import returns `f32`, the codegen emits the reverse:
//!   `i32.reinterpret_f32` — capture as i32 bit-pattern
//!   `i64.extend_i32_u`    — zero-extend to i64 for the guest value model
//!
//! ## String-handle lowering for host calls
//!
//! A string handle in the guest is `(offset << 32) | len`.  Host imports that
//! take `StringHandle` params receive a `(ptr:i32, len:i32)` pair:
//!   `i32.wrap_i64`               — low 32 bits = len
//!   `local.tee $tmp`             — save len
//!   (emit offset computation)    — `local.get $arg` `i64.const 32` `i64.shr_u` `i32.wrap_i64`
//! This is the same approach as kotoba-clj's HasCapability / LlmInfer lowering.

use std::collections::HashMap;

use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, DataSection, EntityType, ExportKind, ExportSection,
    Function, FunctionSection, GlobalSection, GlobalType, ImportSection, Instruction, MemArg,
    MemorySection, MemoryType, Module, TypeSection, ValType,
};

use crate::ast::{Builtin, Expr, HostImport, ParamKind, Program, ReturnKind};
use crate::CljError;

const DATA_BASE:  u32 = 1024;
const HEAP_ALIGN: u32 = 16;
const WASM_PAGE:  u32 = 65536;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn compile(program: &Program) -> Result<Vec<u8>, CljError> {
    // Pass 0: collect string literals into one data blob.
    let literals = collect_literals(program);

    // Pass 1: evaluate def constants + assign function indices.
    let consts     = eval_consts(program)?;
    let host_imports = collect_host_imports(program);
    let import_base  = host_imports.len() as u32;

    let mut types        = TypeSection::new();
    let mut imports      = ImportSection::new();
    let mut import_index = HashMap::<HostImport, u32>::new();

    for (i, imp) in host_imports.iter().enumerate() {
        let (params, results) = host_import_sig(*imp);
        let tidx = types.len();
        types.ty().function(params, results);
        let (module, field) = imp.module_field();
        imports.import(module, field, EntityType::Function(tidx));
        import_index.insert(*imp, i as u32);
    }

    // All guest functions: arity × i64 → i64.
    let mut type_for_arity = HashMap::<usize, u32>::new();
    let mut funcs   = FunctionSection::new();
    let mut exports = ExportSection::new();
    let mut fn_index = HashMap::<String, (u32, usize)>::new();

    for (i, f) in program.functions.iter().enumerate() {
        if fn_index.contains_key(&f.name) {
            return Err(CljError::Codegen(format!("function `{}` defined twice", f.name)));
        }
        let arity = f.params.len();
        let type_idx = *type_for_arity.entry(arity).or_insert_with(|| {
            let idx = types.len();
            types
                .ty()
                .function(std::iter::repeat(ValType::I64).take(arity), [ValType::I64]);
            idx
        });
        funcs.function(type_idx);
        exports.export(&f.name, ExportKind::Func, import_base + i as u32);
        fn_index.insert(f.name.clone(), (import_base + i as u32, arity));
    }

    // cabi_realloc: (old:i32, old_sz:i32, align:i32, new_sz:i32) -> i32
    let realloc_type = types.len();
    types.ty().function(
        [ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        [ValType::I32],
    );
    let realloc_fn = import_base + program.functions.len() as u32;
    funcs.function(realloc_type);
    exports.export("cabi_realloc", ExportKind::Func, realloc_fn);

    // Memory + heap pointer global.
    let heap_start = align_up(DATA_BASE + literals.blob.len() as u32, HEAP_ALIGN);
    let min_pages  = heap_start.div_ceil(WASM_PAGE).max(1) as u64;

    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: min_pages, maximum: None,
        memory64: false, shared: false, page_size_log2: None,
    });
    exports.export("memory", ExportKind::Memory, 0);

    let mut globals = GlobalSection::new();
    globals.global(
        GlobalType { val_type: ValType::I32, mutable: true, shared: false },
        &ConstExpr::i32_const(heap_start as i32),
    );
    const HEAP_GLOBAL: u32 = 0;

    // Pass 2: function bodies.
    let mut code = CodeSection::new();
    for f in &program.functions {
        let mut cg = FnCtx {
            consts:       &consts,
            fn_index:     &fn_index,
            import_index: &import_index,
            literals:     &literals,
            scope: f.params.iter()
                .enumerate()
                .map(|(i, name)| (name.clone(), Local::Param(i as u32)))
                .collect(),
            locals_count: f.params.len() as u32,
            loop_depth: 0,
            ctrl_depth: 0,
            loop_levels: Vec::new(),
            loop_var_locals: Vec::new(),
            heap_global: HEAP_GLOBAL,
            realloc_fn,
        };
        let instrs = cg.emit_body(&f.body)?;
        let mut func = Function::new(cg.local_decls());
        for i in instrs { func.instruction(&i); }
        func.instruction(&Instruction::End);
        code.function(&func);
    }

    // cabi_realloc body — bump allocator.
    {
        // locals: [old_ptr:i32, old_sz:i32, align:i32, new_sz:i32]
        // align heap pointer up, then advance global.
        let align_local = 2u32; // align param
        let new_sz      = 3u32; // new_size param
        let mut f = Function::new([]);
        // current = global + (align-1) & ~(align-1)
        // simplified: just align to HEAP_ALIGN (16) regardless of the align param
        f.instruction(&Instruction::GlobalGet(HEAP_GLOBAL));
        f.instruction(&Instruction::I32Const(HEAP_ALIGN as i32 - 1));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I32Const(-(HEAP_ALIGN as i32)));
        f.instruction(&Instruction::I32And);
        // result = aligned base; also save it
        f.instruction(&Instruction::LocalGet(new_sz));
        // new_global = result + new_sz
        // We need a local to hold the result ptr.
        // Actually let's emit the full body:
        // ptr = (heap + align-1) & ~(align-1)
        // global = ptr + new_sz
        // return ptr
        // Re-emit properly:
        f.instruction(&Instruction::Drop); // drop the partial computation above
        f.instruction(&Instruction::Drop);

        // Redo cleanly.
        f.instruction(&Instruction::GlobalGet(HEAP_GLOBAL));                    // [heap]
        f.instruction(&Instruction::I32Const(HEAP_ALIGN as i32 - 1));          // [heap, 15]
        f.instruction(&Instruction::I32Add);                                    // [heap+15]
        f.instruction(&Instruction::I32Const(-(HEAP_ALIGN as i32)));            // [heap+15, -16]
        f.instruction(&Instruction::I32And);                                    // [aligned]
        // Tee into a local? We don't have extra locals declared for realloc.
        // Use a different structure: ptr then advance global.
        // Stack: [ptr]
        // global = ptr + new_sz
        f.instruction(&Instruction::LocalGet(new_sz));                         // [ptr, new_sz]
        f.instruction(&Instruction::I32Add);                                    // [ptr+new_sz]
        f.instruction(&Instruction::GlobalSet(HEAP_GLOBAL));                    // []
        // Now we need to return ptr but we dropped it.  Use the same pattern again:
        f.instruction(&Instruction::GlobalGet(HEAP_GLOBAL));                    // [new_global]
        f.instruction(&Instruction::LocalGet(new_sz));                         // [new_global, new_sz]
        f.instruction(&Instruction::I32Sub);                                    // [ptr = new_global - new_sz]
        f.instruction(&Instruction::End);
        code.function(&f);
    }

    // Data segment.
    let mut data = DataSection::new();
    if !literals.blob.is_empty() {
        data.active(0, &ConstExpr::i32_const(DATA_BASE as i32), literals.blob.iter().copied());
    }

    // Assemble module.
    let mut module = Module::new();
    module.section(&types);
    module.section(&imports);
    module.section(&funcs);
    module.section(&memories);
    module.section(&globals);
    module.section(&exports);
    module.section(&code);
    if !literals.blob.is_empty() {
        module.section(&data);
    }
    Ok(module.finish())
}

// ---------------------------------------------------------------------------
// Host import WASM type signatures
// ---------------------------------------------------------------------------

/// Build the core-WASM `(params, results)` for a host import, expanding
/// `StringHandle` to two i32s and `F32` to f32.
fn host_import_sig(imp: HostImport) -> (Vec<ValType>, Vec<ValType>) {
    let mut params = Vec::new();
    for kind in imp.param_kinds() {
        match kind {
            ParamKind::I64          => params.push(ValType::I64),
            ParamKind::F32          => params.push(ValType::F32),
            ParamKind::StringHandle => { params.push(ValType::I32); params.push(ValType::I32); }
        }
    }
    let results = match imp.return_kind() {
        ReturnKind::Void => vec![],
        ReturnKind::I32  => vec![ValType::I32],
        ReturnKind::I64  => vec![ValType::I64],
        ReturnKind::F32  => vec![ValType::F32],
    };
    (params, results)
}

// ---------------------------------------------------------------------------
// String literal collection
// ---------------------------------------------------------------------------

struct Literals {
    /// Offsets within `blob` for each string (index matches program order).
    offsets: HashMap<Vec<u8>, u32>,
    blob:    Vec<u8>,
}

fn collect_literals(program: &Program) -> Literals {
    let mut offsets = HashMap::new();
    let mut blob    = Vec::new();
    for f in &program.functions {
        for expr in &f.body {
            collect_str(expr, &mut offsets, &mut blob);
        }
    }
    for d in &program.defs {
        collect_str(&d.value, &mut offsets, &mut blob);
    }
    Literals { offsets, blob }
}

fn collect_str(expr: &Expr, offsets: &mut HashMap<Vec<u8>, u32>, blob: &mut Vec<u8>) {
    match expr {
        Expr::Str(bytes) => {
            offsets.entry(bytes.clone()).or_insert_with(|| {
                let off = blob.len() as u32;
                blob.extend_from_slice(bytes);
                off
            });
        }
        Expr::If { cond, then, els } => {
            collect_str(cond, offsets, blob);
            collect_str(then, offsets, blob);
            collect_str(els, offsets, blob);
        }
        Expr::Let { bindings, body } | Expr::Loop { bindings, body } => {
            for (_, v) in bindings { collect_str(v, offsets, blob); }
            for e in body          { collect_str(e, offsets, blob); }
        }
        Expr::Do(es) | Expr::Recur(es) => {
            for e in es { collect_str(e, offsets, blob); }
        }
        Expr::Builtin { args, .. } | Expr::Call { args, .. } => {
            for e in args { collect_str(e, offsets, blob); }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Constant evaluation (def)
// ---------------------------------------------------------------------------

type Consts = HashMap<String, i64>;

fn eval_consts(program: &Program) -> Result<Consts, CljError> {
    let mut consts = HashMap::new();
    for d in &program.defs {
        let v = eval_const_expr(&d.value, &consts)?;
        consts.insert(d.name.clone(), v);
    }
    Ok(consts)
}

fn eval_const_expr(expr: &Expr, consts: &Consts) -> Result<i64, CljError> {
    match expr {
        Expr::Int(i)   => Ok(*i),
        Expr::Float(f) => Ok(f32::to_bits(*f) as i64),
        Expr::Var(n)   => consts.get(n).copied().ok_or_else(|| {
            CljError::Codegen(format!("def reference to undefined constant `{n}`"))
        }),
        Expr::Builtin { op: Builtin::Add, args } => {
            args.iter().try_fold(0i64, |acc, a| Ok(acc + eval_const_expr(a, consts)?))
        }
        Expr::Builtin { op: Builtin::Sub, args } if args.len() == 1 => {
            Ok(-eval_const_expr(&args[0], consts)?)
        }
        Expr::Builtin { op: Builtin::Sub, args } => {
            let mut it = args.iter();
            let first = eval_const_expr(it.next().unwrap(), consts)?;
            it.try_fold(first, |acc, a| Ok(acc - eval_const_expr(a, consts)?))
        }
        Expr::Builtin { op: Builtin::Mul, args } => {
            args.iter().try_fold(1i64, |acc, a| Ok(acc * eval_const_expr(a, consts)?))
        }
        other => Err(CljError::Codegen(format!(
            "def initialiser must be a constant expression, found {other:?}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Host import collection (dedup)
// ---------------------------------------------------------------------------

fn collect_host_imports(program: &Program) -> Vec<HostImport> {
    let mut seen = std::collections::HashSet::new();
    let mut out  = Vec::new();
    for f in &program.functions {
        for expr in &f.body { scan_imports(expr, &mut seen, &mut out); }
    }
    out
}

fn scan_imports(
    expr: &Expr,
    seen: &mut std::collections::HashSet<HostImport>,
    out:  &mut Vec<HostImport>,
) {
    match expr {
        Expr::Builtin { op, args } => {
            if let Some(imp) = op.host_import() {
                if seen.insert(imp) { out.push(imp); }
            }
            for a in args { scan_imports(a, seen, out); }
        }
        Expr::If { cond, then, els } => {
            scan_imports(cond, seen, out);
            scan_imports(then, seen, out);
            scan_imports(els, seen, out);
        }
        Expr::Let { bindings, body } | Expr::Loop { bindings, body } => {
            for (_, v) in bindings { scan_imports(v, seen, out); }
            for e in body          { scan_imports(e, seen, out); }
        }
        Expr::Do(es) | Expr::Recur(es) => {
            for e in es { scan_imports(e, seen, out); }
        }
        Expr::Call { args, .. } => {
            for a in args { scan_imports(a, seen, out); }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Per-function codegen context
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum Local {
    Param(u32),
    Let(u32),
}

struct FnCtx<'a> {
    consts:       &'a Consts,
    fn_index:     &'a HashMap<String, (u32, usize)>,
    import_index: &'a HashMap<HostImport, u32>,
    literals:     &'a Literals,
    scope:        HashMap<String, Local>,
    locals_count: u32,
    loop_depth:   u32,
    /// Count of WASM control blocks (block/loop/if) currently open at the
    /// emit cursor. Updated in lockstep with every block open/End so `recur`
    /// can compute the relative branch index to its enclosing loop.
    ctrl_depth:   u32,
    /// `ctrl_depth` recorded at each enclosing `loop` (stack; supports nesting).
    loop_levels:  Vec<u32>,
    /// Loop-variable local indices for each enclosing `loop` (parallel to
    /// `loop_levels`), so `recur` rebinds exactly the loop vars — not unrelated
    /// `let` locals that happen to precede them.
    loop_var_locals: Vec<Vec<u32>>,
    heap_global:  u32,
    realloc_fn:   u32,
}

impl<'a> FnCtx<'a> {
    fn alloc_local(&mut self) -> u32 {
        let idx = self.locals_count;
        self.locals_count += 1;
        idx
    }

    fn local_decls(&self) -> Vec<(u32, ValType)> {
        let param_count = self.scope.values().filter(|l| matches!(l, Local::Param(_))).count() as u32;
        let extra = self.locals_count.saturating_sub(param_count);
        if extra > 0 { vec![(extra, ValType::I64)] } else { vec![] }
    }

    fn emit_body(&mut self, body: &[Expr]) -> Result<Vec<Instruction<'static>>, CljError> {
        let mut out = Vec::new();
        for (i, expr) in body.iter().enumerate() {
            let is_last = i == body.len() - 1;
            let instrs = self.emit(expr)?;
            out.extend(instrs);
            // Drop non-final results to keep the stack balanced.
            if !is_last { out.push(Instruction::Drop); }
        }
        Ok(out)
    }

    fn emit(&mut self, expr: &Expr) -> Result<Vec<Instruction<'static>>, CljError> {
        match expr {
            Expr::Int(n)   => Ok(vec![Instruction::I64Const(*n)]),
            Expr::Float(f) => {
                // Compile float literal to its bit-pattern on the i64 stack:
                //   f32.const v → i32.reinterpret_f32 → i64.extend_i32_u
                Ok(vec![
                    Instruction::F32Const(*f),
                    Instruction::I32ReinterpretF32,
                    Instruction::I64ExtendI32U,
                ])
            }
            Expr::Str(bytes) => {
                let off = *self.literals.offsets.get(bytes).ok_or_else(|| {
                    CljError::Codegen("string literal not found in data segment".into())
                })?;
                let abs = DATA_BASE + off;
                let len = bytes.len() as i64;
                // Pack as (abs << 32) | len.
                let handle = ((abs as i64) << 32) | len;
                Ok(vec![Instruction::I64Const(handle)])
            }
            Expr::Var(name) => self.emit_var(name),
            Expr::If { cond, then, els } => self.emit_if(cond, then, els),
            Expr::Let { bindings, body }  => self.emit_let(bindings, body),
            Expr::Do(body)                => self.emit_body(body),
            Expr::Loop { bindings, body } => self.emit_loop(bindings, body),
            Expr::Recur(args)             => self.emit_recur(args),
            Expr::Builtin { op, args }    => self.emit_builtin(*op, args),
            Expr::Call { name, args }     => self.emit_call(name, args),
        }
    }

    fn emit_var(&self, name: &str) -> Result<Vec<Instruction<'static>>, CljError> {
        if let Some(local) = self.scope.get(name) {
            let idx = match local { Local::Param(i) | Local::Let(i) => *i };
            return Ok(vec![Instruction::LocalGet(idx)]);
        }
        if let Some(&v) = self.consts.get(name) {
            return Ok(vec![Instruction::I64Const(v)]);
        }
        Err(CljError::Codegen(format!("unbound variable `{name}`")))
    }

    fn emit_if(
        &mut self,
        cond: &Expr,
        then: &Expr,
        els:  &Expr,
    ) -> Result<Vec<Instruction<'static>>, CljError> {
        let mut out = self.emit(cond)?;
        // Truthy = non-zero i64.
        out.push(Instruction::I64Const(0));
        out.push(Instruction::I64Ne);
        out.push(Instruction::If(BlockType::Result(ValType::I64)));
        self.ctrl_depth += 1; // the if-block is open across both arms
        out.extend(self.emit(then)?);
        out.push(Instruction::Else);
        out.extend(self.emit(els)?);
        out.push(Instruction::End);
        self.ctrl_depth -= 1;
        Ok(out)
    }

    fn emit_let(
        &mut self,
        bindings: &[(String, Expr)],
        body:     &[Expr],
    ) -> Result<Vec<Instruction<'static>>, CljError> {
        let mut out = Vec::new();
        let mut added = Vec::new();
        for (name, val) in bindings {
            let idx = self.alloc_local();
            out.extend(self.emit(val)?);
            out.push(Instruction::LocalSet(idx));
            self.scope.insert(name.clone(), Local::Let(idx));
            added.push(name.clone());
        }
        out.extend(self.emit_body(body)?);
        for name in added { self.scope.remove(&name); }
        Ok(out)
    }

    fn emit_loop(
        &mut self,
        bindings: &[(String, Expr)],
        body:     &[Expr],
    ) -> Result<Vec<Instruction<'static>>, CljError> {
        // Allocate a local for each loop variable.
        let mut init = Vec::new();
        let mut loop_vars: Vec<(String, u32)> = Vec::new();
        for (name, val) in bindings {
            let idx = self.alloc_local();
            init.extend(self.emit(val)?);
            init.push(Instruction::LocalSet(idx));
            self.scope.insert(name.clone(), Local::Let(idx));
            loop_vars.push((name.clone(), idx));
        }

        // Lowering (so a loop yields the body's non-recur value, and `recur`
        // re-enters even when nested inside an `if`):
        //
        //   block $exit (result i64)     ;; carries the final value out
        //     loop $cont                 ;; recur targets this
        //       <body → i64>
        //       br $exit                 ;; deliver the value, leave the loop
        //     end
        //     unreachable                ;; body always br's ($exit or $cont)
        //   end
        //
        // `recur` sets the loop locals then branches to $cont; that path is
        // unreachable afterwards, so the body's value type still checks out.
        self.loop_depth += 1;
        let mut out = init;
        out.push(Instruction::Block(BlockType::Result(ValType::I64)));
        self.ctrl_depth += 1;
        out.push(Instruction::Loop(BlockType::Empty));
        self.ctrl_depth += 1;
        self.loop_levels.push(self.ctrl_depth);
        self.loop_var_locals.push(loop_vars.iter().map(|(_, i)| *i).collect());

        out.extend(self.emit_body(body)?); // leaves the non-recur i64 on the stack
        // Deliver that value to $exit. The block sits exactly one level out from
        // the (balanced) loop body, so the branch index is always 1.
        out.push(Instruction::Br(1));

        self.loop_var_locals.pop();
        self.loop_levels.pop();
        self.ctrl_depth -= 1;
        out.push(Instruction::End); // loop
        out.push(Instruction::Unreachable); // loop never falls through
        self.ctrl_depth -= 1;
        out.push(Instruction::End); // block
        self.loop_depth -= 1;

        for (name, _) in &loop_vars { self.scope.remove(name); }
        Ok(out)
    }

    fn emit_recur(&mut self, args: &[Expr]) -> Result<Vec<Instruction<'static>>, CljError> {
        let loop_level = *self.loop_levels.last().ok_or_else(|| {
            CljError::Codegen("recur outside of a loop".into())
        })?;
        // Exactly the enclosing loop's variable locals (not unrelated lets).
        let let_locals: Vec<u32> = self.loop_var_locals.last().cloned().unwrap_or_default();
        // Clojure recur must rebind EXACTLY the loop vars — under-arity would
        // leave the unlisted vars at their prior values (silent infinite loop /
        // wrong result), so reject any mismatch, not just over-arity.
        if args.len() != let_locals.len() {
            return Err(CljError::Codegen(format!(
                "recur arity {} != loop variables {}",
                args.len(), let_locals.len()
            )));
        }
        let mut out = Vec::new();
        // Evaluate args to tmp locals first so multi-arg recur doesn't clobber
        // a loop var that a later arg still reads.
        let mut tmps = Vec::with_capacity(args.len());
        for a in args {
            out.extend(self.emit(a)?);
            let t = self.alloc_local();
            out.push(Instruction::LocalSet(t));
            tmps.push(t);
        }
        for (t, idx) in tmps.iter().zip(let_locals.iter()) {
            out.push(Instruction::LocalGet(*t));
            out.push(Instruction::LocalSet(*idx));
        }
        // Branch to the enclosing loop. The relative index accounts for any
        // blocks (e.g. an `if`) the recur is nested inside.
        let br_index = self.ctrl_depth - loop_level;
        out.push(Instruction::Br(br_index));
        // Unreachable after the branch; a dummy keeps any enclosing
        // result-typed block's type-checker happy.
        out.push(Instruction::I64Const(0));
        Ok(out)
    }

    // ---- builtins -----------------------------------------------------------

    fn emit_builtin(
        &mut self,
        op:   Builtin,
        args: &[Expr],
    ) -> Result<Vec<Instruction<'static>>, CljError> {
        use Builtin::*;

        // Host imports are handled by a separate path.
        if let Some(imp) = op.host_import() {
            return self.emit_host_call(imp, args);
        }

        let mut out = Vec::new();
        match op {
            Add => {
                out.extend(self.emit(&args[0])?);
                for a in &args[1..] {
                    out.extend(self.emit(a)?);
                    out.push(Instruction::I64Add);
                }
            }
            Sub if args.len() == 1 => {
                out.push(Instruction::I64Const(0));
                out.extend(self.emit(&args[0])?);
                out.push(Instruction::I64Sub);
            }
            Sub => {
                out.extend(self.emit(&args[0])?);
                for a in &args[1..] {
                    out.extend(self.emit(a)?);
                    out.push(Instruction::I64Sub);
                }
            }
            Mul => {
                out.extend(self.emit(&args[0])?);
                for a in &args[1..] {
                    out.extend(self.emit(a)?);
                    out.push(Instruction::I64Mul);
                }
            }
            Div => {
                out.extend(self.emit(&args[0])?);
                out.extend(self.emit(&args[1])?);
                out.push(Instruction::I64DivS);
            }
            Mod => {
                out.extend(self.emit(&args[0])?);
                out.extend(self.emit(&args[1])?);
                out.push(Instruction::I64RemS);
            }
            Inc => {
                out.extend(self.emit(&args[0])?);
                out.push(Instruction::I64Const(1));
                out.push(Instruction::I64Add);
            }
            Dec => {
                out.extend(self.emit(&args[0])?);
                out.push(Instruction::I64Const(1));
                out.push(Instruction::I64Sub);
            }
            Abs => {
                let local = self.alloc_local();
                out.extend(self.emit(&args[0])?);
                out.push(Instruction::LocalTee(local));
                out.push(Instruction::I64Const(0));
                out.push(Instruction::I64LtS);
                out.push(Instruction::If(BlockType::Result(ValType::I64)));
                self.ctrl_depth += 1; // keep the recur br-index invariant intact
                out.push(Instruction::I64Const(0));
                out.push(Instruction::LocalGet(local));
                out.push(Instruction::I64Sub);
                out.push(Instruction::Else);
                out.push(Instruction::LocalGet(local));
                out.push(Instruction::End);
                self.ctrl_depth -= 1;
            }
            Eq => {
                out.extend(self.emit(&args[0])?);
                for a in &args[1..] {
                    out.extend(self.emit(a)?);
                    out.push(Instruction::I64Eq);
                    out.push(Instruction::I64ExtendI32U);
                }
                if args.len() > 2 {
                    // chain: A==B && B==C → simplified as A==B; for multi-arg, fold with AND
                    // TODO: proper chain; for now emit pairwise AND
                }
            }
            NotEq => {
                out.extend(self.emit(&args[0])?);
                out.extend(self.emit(&args[1])?);
                out.push(Instruction::I64Ne);
                out.push(Instruction::I64ExtendI32U);
            }
            Lt => {
                out.extend(self.emit(&args[0])?);
                out.extend(self.emit(&args[1])?);
                out.push(Instruction::I64LtS);
                out.push(Instruction::I64ExtendI32U);
            }
            Gt => {
                out.extend(self.emit(&args[0])?);
                out.extend(self.emit(&args[1])?);
                out.push(Instruction::I64GtS);
                out.push(Instruction::I64ExtendI32U);
            }
            Le => {
                out.extend(self.emit(&args[0])?);
                out.extend(self.emit(&args[1])?);
                out.push(Instruction::I64LeS);
                out.push(Instruction::I64ExtendI32U);
            }
            Ge => {
                out.extend(self.emit(&args[0])?);
                out.extend(self.emit(&args[1])?);
                out.push(Instruction::I64GeS);
                out.push(Instruction::I64ExtendI32U);
            }
            Zero => {
                out.extend(self.emit(&args[0])?);
                out.push(Instruction::I64Eqz);
                out.push(Instruction::I64ExtendI32U);
            }
            Pos => {
                out.extend(self.emit(&args[0])?);
                out.push(Instruction::I64Const(0));
                out.push(Instruction::I64GtS);
                out.push(Instruction::I64ExtendI32U);
            }
            Neg => {
                out.extend(self.emit(&args[0])?);
                out.push(Instruction::I64Const(0));
                out.push(Instruction::I64LtS);
                out.push(Instruction::I64ExtendI32U);
            }
            Not => {
                out.extend(self.emit(&args[0])?);
                out.push(Instruction::I64Eqz);
                out.push(Instruction::I64ExtendI32U);
            }
            And => {
                // Short-circuit: (and a b …)
                // Emit as nested ifs: (if a (if b … 0) 0) returning last truthy val
                out.extend(self.emit(&args[0])?);
                let tmp = self.alloc_local();
                out.push(Instruction::LocalTee(tmp));
                out.push(Instruction::I64Const(0));
                out.push(Instruction::I64Ne);
                out.push(Instruction::If(BlockType::Result(ValType::I64)));
                self.ctrl_depth += 1;
                if args.len() == 1 {
                    out.push(Instruction::LocalGet(tmp));
                } else {
                    let rest = Expr::Builtin { op: And, args: args[1..].to_vec() };
                    out.extend(self.emit(&rest)?);
                }
                out.push(Instruction::Else);
                out.push(Instruction::LocalGet(tmp));
                out.push(Instruction::End);
                self.ctrl_depth -= 1;
            }
            Or => {
                out.extend(self.emit(&args[0])?);
                let tmp = self.alloc_local();
                out.push(Instruction::LocalTee(tmp));
                out.push(Instruction::I64Const(0));
                out.push(Instruction::I64Ne);
                out.push(Instruction::If(BlockType::Result(ValType::I64)));
                self.ctrl_depth += 1;
                out.push(Instruction::LocalGet(tmp));
                out.push(Instruction::Else);
                if args.len() == 1 {
                    out.push(Instruction::LocalGet(tmp));
                } else {
                    let rest = Expr::Builtin { op: Or, args: args[1..].to_vec() };
                    out.extend(self.emit(&rest)?);
                }
                out.push(Instruction::End);
                self.ctrl_depth -= 1;
            }
            StrLen => {
                // Low 32 bits = len.
                out.extend(self.emit(&args[0])?);
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::I64ExtendI32U);
            }
            ByteAt => {
                // Extract (ptr + i) byte from memory.
                // handle = (ptr<<32)|len; ptr = handle >> 32
                let handle = self.alloc_local();
                out.extend(self.emit(&args[0])?);
                out.push(Instruction::LocalSet(handle));
                out.extend(self.emit(&args[1])?);   // i (i64)
                out.push(Instruction::I32WrapI64);   // i as i32
                out.push(Instruction::LocalGet(handle));
                out.push(Instruction::I64Const(32));
                out.push(Instruction::I64ShrU);
                out.push(Instruction::I32WrapI64);   // ptr as i32
                out.push(Instruction::I32Add);       // ptr + i
                out.push(Instruction::I32Load8U(MemArg { offset: 0, align: 0, memory_index: 0 }));
                out.push(Instruction::I64ExtendI32U);
            }
            BytesAlloc => {
                // Allocate a buffer: [cap:i32@0, len:i32@4, data:@8]
                // Returns a buffer handle (ptr as i64 — NOT a string handle; ptr only)
                let cap_local = self.alloc_local();  // i64
                let ptr_local = self.alloc_local();  // i64
                // cap = args[0]
                out.extend(self.emit(&args[0])?);    // i64
                out.push(Instruction::LocalSet(cap_local));
                // ptr = heap_global
                out.push(Instruction::GlobalGet(self.heap_global)); // i32
                out.push(Instruction::I64ExtendI32U);
                out.push(Instruction::LocalSet(ptr_local));
                // global = ptr + 8 + cap
                out.push(Instruction::LocalGet(ptr_local));
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::I32Const(8));
                out.push(Instruction::I32Add);
                out.push(Instruction::LocalGet(cap_local));
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::I32Add);
                out.push(Instruction::GlobalSet(self.heap_global));
                // store cap at [ptr+0]
                out.push(Instruction::LocalGet(ptr_local));
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::LocalGet(cap_local));
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));
                // store len=0 at [ptr+4]
                out.push(Instruction::LocalGet(ptr_local));
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::I32Const(0));
                out.push(Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));
                // return ptr as i64
                out.push(Instruction::LocalGet(ptr_local));
            }
            ByteAppend => {
                // buf handle (ptr), byte value
                let ptr_local  = self.alloc_local(); // i64
                let byte_local = self.alloc_local(); // i64
                let len_local  = self.alloc_local(); // i64
                // ptr = args[0]
                out.extend(self.emit(&args[0])?);    // i64
                out.push(Instruction::LocalSet(ptr_local));
                // byte = args[1]
                out.extend(self.emit(&args[1])?);    // i64
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::I64ExtendI32U);
                out.push(Instruction::LocalSet(byte_local));
                // len = [ptr+4]
                out.push(Instruction::LocalGet(ptr_local));
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::I32Load(MemArg { offset: 4, align: 2, memory_index: 0 }));
                out.push(Instruction::I64ExtendI32U);
                out.push(Instruction::LocalSet(len_local));
                // store byte at [ptr+8+len]
                out.push(Instruction::LocalGet(ptr_local));
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::I32Const(8));
                out.push(Instruction::I32Add);
                out.push(Instruction::LocalGet(len_local));
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::I32Add);
                out.push(Instruction::LocalGet(byte_local));
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::I32Store8(MemArg { offset: 0, align: 0, memory_index: 0 }));
                // [ptr+4] = len + 1
                out.push(Instruction::LocalGet(ptr_local));
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::LocalGet(len_local));
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::I32Const(1));
                out.push(Instruction::I32Add);
                out.push(Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));
                // return ptr as i64 (buf handle unchanged)
                out.push(Instruction::LocalGet(ptr_local));
            }
            BytesLen => {
                out.extend(self.emit(&args[0])?);
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::I32Load(MemArg { offset: 4, align: 2, memory_index: 0 }));
                out.push(Instruction::I64ExtendI32U);
            }
            BytesFinish => {
                // Convert a buffer handle → string handle ((data_ptr << 32) | len)
                let ptr_local = self.alloc_local(); // i64
                // ptr = args[0]
                out.extend(self.emit(&args[0])?);   // i64
                out.push(Instruction::LocalSet(ptr_local));
                // data_ptr = ptr + 8  (high 32 bits of string handle)
                out.push(Instruction::LocalGet(ptr_local));
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::I32Const(8));
                out.push(Instruction::I32Add);
                out.push(Instruction::I64ExtendI32U);
                out.push(Instruction::I64Const(32));
                out.push(Instruction::I64Shl);
                // len = [ptr+4]  (low 32 bits of string handle)
                out.push(Instruction::LocalGet(ptr_local));
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::I32Load(MemArg { offset: 4, align: 2, memory_index: 0 }));
                out.push(Instruction::I64ExtendI32U);
                // handle = (data_ptr << 32) | len
                out.push(Instruction::I64Or);
            }
            Alloc => {
                // (alloc n) — bump-allocate n bytes; return ptr as i64.
                let n_local   = self.alloc_local(); // i64
                let ptr_local = self.alloc_local(); // i64
                // n = align(args[0], HEAP_ALIGN)
                out.extend(self.emit(&args[0])?);    // i64
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::I32Const(HEAP_ALIGN as i32 - 1));
                out.push(Instruction::I32Add);
                out.push(Instruction::I32Const(-(HEAP_ALIGN as i32)));
                out.push(Instruction::I32And);       // i32
                out.push(Instruction::I64ExtendI32U);
                out.push(Instruction::LocalSet(n_local));
                // ptr = heap_global
                out.push(Instruction::GlobalGet(self.heap_global)); // i32
                out.push(Instruction::I64ExtendI32U);
                out.push(Instruction::LocalSet(ptr_local));
                // global = ptr + n
                out.push(Instruction::LocalGet(ptr_local));
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::LocalGet(n_local));
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::I32Add);
                out.push(Instruction::GlobalSet(self.heap_global));
                // return ptr as i64
                out.push(Instruction::LocalGet(ptr_local));
            }
            Load64 => {
                out.extend(self.emit(&args[0])?);
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::I64Load(MemArg { offset: 0, align: 3, memory_index: 0 }));
            }
            Store64 => {
                out.extend(self.emit(&args[0])?); // addr
                out.push(Instruction::I32WrapI64);
                out.extend(self.emit(&args[1])?); // val
                out.push(Instruction::I64Store(MemArg { offset: 0, align: 3, memory_index: 0 }));
                out.push(Instruction::I64Const(0)); // returns void (0)
            }
            Load32 => {
                out.extend(self.emit(&args[0])?);
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::I32Load(MemArg { offset: 0, align: 2, memory_index: 0 }));
                out.push(Instruction::I64ExtendI32U);
            }
            Store32 => {
                out.extend(self.emit(&args[0])?); // addr
                out.push(Instruction::I32WrapI64);
                out.extend(self.emit(&args[1])?); // val
                out.push(Instruction::I32WrapI64);
                out.push(Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));
                out.push(Instruction::I64Const(0));
            }
            F32Bits => {
                // Identity — f32 bit-pattern already in i64 value model.
                out.extend(self.emit(&args[0])?);
            }
            BitsF32 => {
                // Also identity at this level; callers use this for documentation.
                out.extend(self.emit(&args[0])?);
            }
            // Game builtins with host_import() are handled above.
            op => {
                return Err(CljError::Codegen(format!(
                    "unhandled builtin {op:?} — should have been covered by host_import path"
                )));
            }
        }
        Ok(out)
    }

    /// Emit a host-import call, handling ParamKind lowering and ReturnKind lifting.
    fn emit_host_call(
        &mut self,
        imp:  HostImport,
        args: &[Expr],
    ) -> Result<Vec<Instruction<'static>>, CljError> {
        let fn_idx = *self.import_index.get(&imp).ok_or_else(|| {
            CljError::Codegen(format!("host import {imp:?} not in import index"))
        })?;
        let param_kinds = imp.param_kinds();

        // Verify caller arity matches host params (accounting for StringHandle→2).
        if args.len() != param_kinds.len() {
            return Err(CljError::Codegen(format!(
                "host call {imp:?} expects {} args, got {}",
                param_kinds.len(), args.len()
            )));
        }

        let mut out = Vec::new();
        for (arg, kind) in args.iter().zip(param_kinds.iter()) {
            out.extend(self.emit(arg)?); // i64 on stack
            match kind {
                ParamKind::I64 => {}  // leave as i64
                ParamKind::F32 => {
                    // i64 → i32 (low bits) → f32
                    out.push(Instruction::I32WrapI64);
                    out.push(Instruction::F32ReinterpretI32);
                }
                ParamKind::StringHandle => {
                    // (ptr<<32)|len — split into (i32 ptr, i32 len) for the host.
                    // Stack has the handle (i64). We need to push ptr first, then len.
                    let handle_local = self.alloc_local();
                    out.push(Instruction::LocalTee(handle_local));
                    // ptr = handle >> 32 (as i32)
                    out.push(Instruction::I64Const(32));
                    out.push(Instruction::I64ShrU);
                    out.push(Instruction::I32WrapI64);
                    // len = handle & 0xFFFF_FFFF (as i32)
                    out.push(Instruction::LocalGet(handle_local));
                    out.push(Instruction::I32WrapI64);
                }
            }
        }

        out.push(Instruction::Call(fn_idx));

        // Lift return value to i64.
        match imp.return_kind() {
            ReturnKind::Void => out.push(Instruction::I64Const(0)),
            ReturnKind::I32  => out.push(Instruction::I64ExtendI32U),
            ReturnKind::I64  => {}
            ReturnKind::F32  => {
                out.push(Instruction::I32ReinterpretF32);
                out.push(Instruction::I64ExtendI32U);
            }
        }
        Ok(out)
    }

    fn emit_call(
        &mut self,
        name: &str,
        args: &[Expr],
    ) -> Result<Vec<Instruction<'static>>, CljError> {
        let (fn_idx, arity) = self.fn_index.get(name).copied().ok_or_else(|| {
            CljError::Codegen(format!("call to undefined function `{name}`"))
        })?;
        if args.len() != arity {
            return Err(CljError::Codegen(format!(
                "function `{name}` called with {} args, expected {arity}",
                args.len()
            )));
        }
        let mut out = Vec::new();
        for a in args { out.extend(self.emit(a)?); }
        out.push(Instruction::Call(fn_idx));
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

fn align_up(val: u32, align: u32) -> u32 {
    (val + align - 1) & !(align - 1)
}
