//! Resumable ESC virtual machine and script scheduler.

use crate::ast::*;
use std::collections::HashMap;
use std::rc::Rc;

/// Engine ticks per `Sleep` millisecond: `Sleep(ms)` waits `ms / 14` ticks
/// (`FUN_0040b440`), so one tick is 14 ms.
pub const MS_PER_TICK: u32 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepMode {
    /// One statement per tick; latent host calls do not block.
    #[default]
    Normal,
    /// One statement per tick; latent host calls block until complete.
    Single,
    /// Run statements until the mode changes or the script ends.
    Multi,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i32),
    Str(Rc<str>),
}

impl Value {
    /// Integer view. Strings evaluate to the engine's (non-null) string buffer
    /// pointer; we use 1 so that truthiness tests behave the same.
    pub fn int(&self) -> i32 {
        match self {
            Value::Int(i) => *i,
            Value::Str(_) => 1,
        }
    }

    pub fn str(&self) -> &str {
        match self {
            Value::Str(s) => s,
            Value::Int(_) => "",
        }
    }
}

impl Default for Value {
    fn default() -> Self {
        Value::Int(0)
    }
}

/// Result of a host-function call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostResult {
    /// Finished, with a return value.
    Done(i32),
    /// Returned a value but wants to be polled again next tick (e.g. a walk).
    Latent(i32),
    /// No such host function.
    Unknown,
}

/// Information passed with each host call.
pub struct CallCtx<'a> {
    pub script: ScriptId,
    pub script_name: &'a str,
    /// `true` when re-polling a pending latent call.
    pub poll: bool,
    pub tick: u32,
}

/// Engine functions callable from scripts (everything except the script-control
/// builtins, which the [`Scheduler`] implements itself).
pub trait Host {
    fn call(&mut self, ctx: &CallCtx, name: &str, args: &[Value]) -> HostResult;
    /// Load an `.esc` source by script-relative path (e.g. `"effekte\\upgrade2.esc"`).
    fn load_source(&mut self, path: &str) -> Option<String>;
    fn log(&mut self, _script: &str, _msg: &str) {}
}

// ---------------------------------------------------------------------------
// Compilation to a flat, resumable instruction list per function.

#[derive(Debug, Clone)]
enum Instr {
    /// Simple statement; costs one step.
    Exec(Stmt),
    /// Script-function call; costs one step.
    Call(usize, Vec<Expr>),
    /// Evaluate condition; jump if false. Costs one step.
    JumpUnless(Expr, usize),
    /// Unconditional jump (`else` skip, loop back-edge); costs one step like the
    /// engine's `}` / `else` tokens.
    Jump(usize),
    Return(Option<Expr>),
}

#[derive(Debug, Clone)]
struct CompiledFn {
    params: Vec<(Type, String)>,
    code: Vec<Instr>,
}

#[derive(Debug, Clone)]
pub struct Compiled {
    functions: Vec<CompiledFn>,
    by_name: HashMap<String, usize>,
    globals: Vec<Stmt>,
    pub includes: Vec<String>,
}

fn compile_block(stmts: &[Stmt], fns: &HashMap<String, usize>, out: &mut Vec<Instr>) {
    for s in stmts {
        match s {
            Stmt::Block(b) => compile_block(b, fns, out),
            Stmt::If { cond, then, otherwise } => {
                let j = out.len();
                out.push(Instr::JumpUnless(cond.clone(), 0));
                compile_block(then, fns, out);
                if let Some(o) = otherwise {
                    let skip = out.len();
                    out.push(Instr::Jump(0));
                    let else_start = out.len();
                    compile_block(o, fns, out);
                    let end = out.len();
                    out[j] = Instr::JumpUnless(cond.clone(), else_start);
                    out[skip] = Instr::Jump(end);
                } else {
                    let end = out.len();
                    out[j] = Instr::JumpUnless(cond.clone(), end);
                }
            }
            Stmt::While { cond, body } => {
                let top = out.len();
                out.push(Instr::JumpUnless(cond.clone(), 0));
                compile_block(body, fns, out);
                out.push(Instr::Jump(top));
                let end = out.len();
                out[top] = Instr::JumpUnless(cond.clone(), end);
            }
            Stmt::Return(e) => out.push(Instr::Return(e.clone())),
            Stmt::Expr(Expr::Chain(c)) if c.len() == 1 => match &c[0].1 {
                Operand::Call(name, args) if fns.contains_key(name) => out.push(Instr::Call(fns[name], args.clone())),
                _ => out.push(Instr::Exec(s.clone())),
            },
            _ => out.push(Instr::Exec(s.clone())),
        }
    }
}

impl Compiled {
    pub fn new(programs: &[Program]) -> Self {
        let mut by_name = HashMap::new();
        let mut all_fns = Vec::new();
        let mut globals = Vec::new();
        let mut includes = Vec::new();
        for p in programs {
            for f in &p.functions {
                by_name.entry(f.name.clone()).or_insert(all_fns.len());
                all_fns.push(f.clone());
            }
            globals.extend(p.globals.iter().cloned());
            includes.extend(p.includes.iter().cloned());
        }
        let functions = all_fns
            .iter()
            .map(|f| {
                let mut code = Vec::new();
                compile_block(&f.body, &by_name, &mut code);
                CompiledFn { params: f.params.clone(), code }
            })
            .collect();
        Compiled { functions, by_name, globals, includes }
    }

    pub fn has_function(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }
}

// ---------------------------------------------------------------------------
// Runtime.

#[derive(Debug, Clone)]
struct Var {
    ty: Type,
    cells: Vec<Value>,
}

impl Var {
    fn new(ty: Type, size: usize) -> Self {
        let init = if ty == Type::String { Value::Str(Rc::from("")) } else { Value::Int(0) };
        Var { ty, cells: vec![init; size.max(1)] }
    }

    fn store(&mut self, idx: usize, v: Value) {
        let Some(slot) = self.cells.get_mut(idx) else { return };
        *slot = match self.ty {
            Type::String => match v {
                Value::Str(s) => Value::Str(s),
                // Assigning an int to a string copies from the string buffer only
                // when the value is non-zero (FUN_00423770); keep the old value.
                Value::Int(_) => return,
            },
            Type::Byte => Value::Int(v.int() as i8 as i32),
            _ => Value::Int(v.int()),
        };
    }
}

#[derive(Debug)]
struct Frame {
    func: usize,
    pc: usize,
    locals: HashMap<String, Var>,
}

#[derive(Debug, Clone)]
enum Pending {
    Sleep { until: Option<u32> },
    Host { name: String, args: Vec<Value> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScriptId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Running,
    /// Running the script's `exit()` handler.
    Exiting,
    Done,
}

pub struct Script {
    pub id: ScriptId,
    pub name: String,
    /// Owning context (scene/location); `KillLocalScripts` stops siblings with the
    /// same context. Engine field `+0x104`.
    pub context: i32,
    code: Rc<Compiled>,
    globals: HashMap<String, Var>,
    frames: Vec<Frame>,
    pub mode: StepMode,
    pub state: State,
    pending: Option<Pending>,
    /// Value of the last `return` (engine global `DAT_006611e0`).
    pub last_return: i32,
}

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("script not found: {0}")]
    NotFound(String),
    #[error("parse error in {0}: {1}")]
    Parse(String, crate::parser::ParseError),
    #[error("no entrypoint (main) in {0}")]
    NoMain(String),
    #[error("wrong parameter count for {0}")]
    WrongArgs(String),
}

impl Script {
    fn new(id: ScriptId, name: String, context: i32, code: Rc<Compiled>) -> Self {
        let mut globals = HashMap::new();
        for g in &code.globals {
            if let Stmt::Decl { ty, name, size, init } = g {
                let mut v = Var::new(*ty, size.unwrap_or(1) as usize);
                if let Some(i) = init {
                    v.store(0, Value::Int(*i));
                }
                globals.insert(name.clone(), v);
            }
        }
        Script {
            id,
            name,
            context,
            code,
            globals,
            frames: Vec::new(),
            mode: StepMode::Normal,
            state: State::Running,
            pending: None,
            last_return: 0,
        }
    }

    fn enter(&mut self, func: usize, args: Vec<Value>) {
        let f = &self.code.functions[func];
        let mut locals = HashMap::new();
        for (i, (ty, name)) in f.params.iter().enumerate() {
            let mut v = Var::new(*ty, 1);
            v.store(0, args.get(i).cloned().unwrap_or_default());
            locals.insert(name.clone(), v);
        }
        self.frames.push(Frame { func, pc: 0, locals });
    }

    fn var_mut(&mut self, name: &str) -> Option<&mut Var> {
        if let Some(f) = self.frames.last_mut() {
            if f.locals.contains_key(name) {
                return f.locals.get_mut(name);
            }
        }
        self.globals.get_mut(name)
    }

    fn var(&self, name: &str) -> Option<&Var> {
        self.frames.last().and_then(|f| f.locals.get(name)).or_else(|| self.globals.get(name))
    }

    pub fn global_int(&self, name: &str, idx: usize) -> Option<i32> {
        self.globals.get(name).and_then(|v| v.cells.get(idx)).map(Value::int)
    }

    pub fn is_done(&self) -> bool {
        self.state == State::Done
    }
}

/// Owns all running scripts and drives them once per engine tick.
pub struct Scheduler {
    scripts: Vec<Script>,
    next_id: u32,
    cache: HashMap<String, Rc<Compiled>>,
    pub tick: u32,
    /// Context assigned to scripts started from the engine (not from scripts).
    pub current_context: i32,
}

struct Effects {
    spawn: Vec<(String, Vec<Value>, i32)>,
    stop: Vec<ScriptId>,
    kill_local: Option<(i32, ScriptId)>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        Scheduler { scripts: Vec::new(), next_id: 1, cache: HashMap::new(), tick: 0, current_context: 0 }
    }

    pub fn scripts(&self) -> &[Script] {
        &self.scripts
    }

    pub fn get(&self, id: ScriptId) -> Option<&Script> {
        self.scripts.iter().find(|s| s.id == id)
    }

    fn compile(&mut self, host: &mut dyn Host, path: &str) -> Result<Rc<Compiled>, RunError> {
        let key = path.to_lowercase().replace('/', "\\");
        if let Some(c) = self.cache.get(&key) {
            return Ok(c.clone());
        }
        let mut programs = Vec::new();
        let mut queue = vec![path.to_string()];
        let mut seen = std::collections::HashSet::new();
        while let Some(p) = queue.pop() {
            if !seen.insert(p.to_lowercase()) {
                continue;
            }
            let src = host.load_source(&p).ok_or_else(|| RunError::NotFound(p.clone()))?;
            let prog = crate::parser::parse(&src).map_err(|e| RunError::Parse(p.clone(), e))?;
            queue.extend(prog.includes.iter().cloned());
            // Statement-level #include inside functions.
            fn collect(stmts: &[Stmt], out: &mut Vec<String>) {
                for s in stmts {
                    match s {
                        Stmt::Include(i) => out.push(i.clone()),
                        Stmt::Block(b) | Stmt::While { body: b, .. } => collect(b, out),
                        Stmt::If { then, otherwise, .. } => {
                            collect(then, out);
                            if let Some(o) = otherwise {
                                collect(o, out);
                            }
                        }
                        _ => {}
                    }
                }
            }
            for f in &prog.functions {
                collect(&f.body, &mut queue);
            }
            programs.push(prog);
        }
        let c = Rc::new(Compiled::new(&programs));
        self.cache.insert(key, c.clone());
        Ok(c)
    }

    /// Start a script's `main` with the given arguments (`evt_RunScript`).
    pub fn run(&mut self, host: &mut dyn Host, path: &str, args: Vec<Value>, context: i32) -> Result<ScriptId, RunError> {
        let code = self.compile(host, path)?;
        let main = *code.by_name.get("main").ok_or_else(|| RunError::NoMain(path.into()))?;
        if code.functions[main].params.len() != args.len() {
            return Err(RunError::WrongArgs(path.into()));
        }
        let id = ScriptId(self.next_id);
        self.next_id += 1;
        let mut s = Script::new(id, path.to_string(), context, code);
        s.enter(main, args);
        self.scripts.push(s);
        Ok(id)
    }

    pub fn find(&self, path: &str) -> Option<ScriptId> {
        self.scripts.iter().find(|s| s.name.eq_ignore_ascii_case(path) && !s.is_done()).map(|s| s.id)
    }

    /// Stop a script: runs its `exit()` handler (in multi-step mode, starting
    /// immediately) unless it is already exiting (`FUN_00423180`).
    pub fn stop(&mut self, host: &mut dyn Host, id: ScriptId) {
        let Some(i) = self.scripts.iter().position(|s| s.id == id) else { return };
        Self::begin_exit(&mut self.scripts[i]);
        if self.scripts[i].state == State::Exiting {
            self.run_script(host, i);
        }
    }

    fn begin_exit(s: &mut Script) {
        if s.state != State::Running {
            s.state = State::Done;
            s.frames.clear();
            return;
        }
        s.frames.clear();
        s.pending = None;
        match s.code.by_name.get("exit") {
            Some(&f) => {
                s.state = State::Exiting;
                s.mode = StepMode::Multi;
                s.enter(f, Vec::new());
            }
            None => s.state = State::Done,
        }
    }

    /// Advance all scripts by one engine tick (`FUN_004241e0`).
    pub fn tick(&mut self, host: &mut dyn Host) {
        self.tick = self.tick.wrapping_add(1);
        let mut i = 0;
        while i < self.scripts.len() {
            if !self.scripts[i].is_done() {
                self.run_script(host, i);
            }
            i += 1;
        }
        self.scripts.retain(|s| !s.is_done());
    }

    fn run_script(&mut self, host: &mut dyn Host, i: usize) {
        let tick = self.tick;
        // Re-poll a pending latent call first (FUN_00423eb0).
        if let Some(p) = self.scripts[i].pending.clone() {
            let still = match p {
                Pending::Sleep { until } => until.is_none_or(|u| tick < u),
                Pending::Host { name, args } => {
                    let s = &self.scripts[i];
                    let ctx = CallCtx { script: s.id, script_name: &s.name, poll: true, tick };
                    matches!(host.call(&ctx, &name, &args), HostResult::Latent(_))
                }
            };
            if still {
                return;
            }
            self.scripts[i].pending = None;
        }
        let mut guard = 0;
        loop {
            let mut fx = Effects { spawn: Vec::new(), stop: Vec::new(), kill_local: None };
            self.step(host, i, &mut fx);
            self.apply(host, fx);
            let s = &self.scripts[i];
            guard += 1;
            if s.mode != StepMode::Multi || s.is_done() || guard > 100_000 {
                break;
            }
            // Deviation: the engine keeps going after a Sleep inside #multistep and
            // can clobber the pending call; we yield instead.
            if matches!(s.pending, Some(Pending::Sleep { .. })) {
                break;
            }
        }
    }

    fn apply(&mut self, host: &mut dyn Host, fx: Effects) {
        for (path, args, ctx) in fx.spawn {
            if let Err(e) = self.run(host, &path, args, ctx) {
                host.log(&path, &e.to_string());
            }
        }
        for id in fx.stop {
            self.stop(host, id);
        }
        if let Some((ctx, me)) = fx.kill_local {
            let ids: Vec<ScriptId> =
                self.scripts.iter().filter(|s| s.context == ctx && s.id != me && !s.is_done()).map(|s| s.id).collect();
            for id in ids {
                self.stop(host, id);
            }
        }
    }

    /// Execute one statement of script `i` (`FUN_00423be0`).
    fn step(&mut self, host: &mut dyn Host, i: usize, fx: &mut Effects) {
        let tick = self.tick;
        let finished_fn = {
            let s = &self.scripts[i];
            match s.frames.last() {
                None => true,
                Some(f) => f.pc >= s.code.functions[f.func].code.len(),
            }
        };
        if finished_fn {
            let s = &mut self.scripts[i];
            s.frames.pop();
            if s.frames.is_empty() {
                Self::begin_exit(s);
            }
            return;
        }
        let (func, pc) = {
            let f = self.scripts[i].frames.last().unwrap();
            (f.func, f.pc)
        };
        let code = self.scripts[i].code.clone();
        let instr = &code.functions[func].code[pc];
        self.scripts[i].frames.last_mut().unwrap().pc += 1;
        let running: Vec<(String, ScriptId)> =
            self.scripts.iter().filter(|s| !s.is_done()).map(|s| (s.name.clone(), s.id)).collect();
        let mut ev = Eval { sched_tick: tick, host, fx, latent: None, running: &running };
        match instr {
            Instr::Exec(stmt) => ev.exec(&mut self.scripts[i], stmt),
            Instr::Call(f, args) => {
                let vals: Vec<Value> = args.iter().map(|a| ev.eval(&mut self.scripts[i], a)).collect();
                self.scripts[i].enter(*f, vals);
            }
            Instr::JumpUnless(cond, target) => {
                if ev.eval(&mut self.scripts[i], cond).int() == 0 {
                    self.scripts[i].frames.last_mut().unwrap().pc = *target;
                }
            }
            Instr::Jump(target) => self.scripts[i].frames.last_mut().unwrap().pc = *target,
            Instr::Return(e) => {
                if let Some(e) = e {
                    let v = ev.eval(&mut self.scripts[i], e).int();
                    self.scripts[i].last_return = v;
                }
                let s = &mut self.scripts[i];
                s.frames.pop();
                if s.frames.is_empty() {
                    Self::begin_exit(s);
                }
            }
        }
        // Record a latent host call (FUN_00423910): blocks in single-step mode;
        // Sleep blocks always.
        if let Some(p) = ev.latent.take() {
            let s = &mut self.scripts[i];
            let block = matches!(p, Pending::Sleep { .. }) || s.mode == StepMode::Single;
            if block && s.state != State::Done {
                s.pending = Some(p);
            }
        }
    }
}

struct Eval<'a> {
    running: &'a [(String, ScriptId)],
    sched_tick: u32,
    host: &'a mut dyn Host,
    fx: &'a mut Effects,
    latent: Option<Pending>,
}

impl Eval<'_> {
    fn exec(&mut self, s: &mut Script, stmt: &Stmt) {
        match stmt {
            Stmt::Decl { ty, name, size, init } => {
                let f = s.frames.last_mut().unwrap();
                let mut v = Var::new(*ty, size.unwrap_or(1) as usize);
                if let Some(i) = init {
                    v.store(0, Value::Int(*i));
                }
                f.locals.entry(name.clone()).or_insert(v);
            }
            Stmt::Assign { name, index, value } => {
                let idx = index.as_ref().map(|e| self.eval(s, e).int()).unwrap_or(0);
                let v = self.eval(s, value);
                if let Some(var) = s.var_mut(name) {
                    if idx >= 0 {
                        var.store(idx as usize, v);
                    }
                } else {
                    self.host.log(&s.name, &format!("Unknown symbol: '{name}'"));
                }
            }
            Stmt::Incr { name, index, delta } => {
                let idx = index.as_ref().map(|e| self.eval(s, e).int()).unwrap_or(0);
                if let Some(var) = s.var_mut(name) {
                    if matches!(var.ty, Type::Int | Type::Byte) {
                        if let Some(c) = var.cells.get(idx.max(0) as usize).cloned() {
                            var.store(idx as usize, Value::Int(c.int().wrapping_add(*delta)));
                        }
                    }
                }
            }
            Stmt::Expr(e) => {
                self.eval(s, e);
            }
            Stmt::Mode(m) => s.mode = *m,
            Stmt::Unknown(name) => {
                self.host.log(&s.name, &format!("Unknown symbol : '{name}'"));
                self.fx.stop.push(s.id);
            }
            Stmt::Include(_) | Stmt::Block(_) | Stmt::If { .. } | Stmt::While { .. } | Stmt::Return(_) => {}
        }
    }

    fn eval(&mut self, s: &mut Script, e: &Expr) -> Value {
        match e {
            Expr::Chain(ops) => {
                let mut acc = Value::Int(0);
                for (k, (op, operand)) in ops.iter().enumerate() {
                    let v = self.operand(s, operand);
                    if k == 0 {
                        acc = v;
                        continue;
                    }
                    let (a, b) = (acc.int(), v.int());
                    acc = Value::Int(match op {
                        BinOp::Add => a.wrapping_add(b),
                        BinOp::Sub => a.wrapping_sub(b),
                        BinOp::Mul => a.wrapping_mul(b),
                        BinOp::Div => {
                            if b == 0 {
                                0
                            } else {
                                a.wrapping_div(b)
                            }
                        }
                        BinOp::BitOr => a | b,
                        BinOp::BitAnd => a & b,
                        BinOp::Replace => {
                            acc = v;
                            continue;
                        }
                    });
                }
                acc
            }
            Expr::Cmp(l, op, r) => {
                let (lv, rv) = (self.eval(s, l), self.eval(s, r));
                let r = match (&lv, &rv) {
                    // Two strings both live in the engine's single string buffer:
                    // pointer comparison makes them always equal.
                    (Value::Str(_), Value::Str(_)) => match op {
                        CmpOp::Eq | CmpOp::Le | CmpOp::Ge => true,
                        _ => false,
                    },
                    _ => {
                        let (a, b) = (lv.int(), rv.int());
                        match op {
                            CmpOp::Eq => a == b,
                            CmpOp::Ne => a != b,
                            CmpOp::Lt => a < b,
                            CmpOp::Le => a <= b,
                            CmpOp::Gt => a > b,
                            CmpOp::Ge => a >= b,
                        }
                    }
                };
                Value::Int(r as i32)
            }
            Expr::Logic(l, op, r) => {
                let (a, b) = (self.eval(s, l).int(), self.eval(s, r).int());
                Value::Int(match op {
                    LogicOp::And => a & b,
                    LogicOp::Or => a | b,
                })
            }
        }
    }

    fn operand(&mut self, s: &mut Script, o: &Operand) -> Value {
        match o {
            Operand::Int(i) => Value::Int(*i),
            Operand::Str(t) => Value::Str(Rc::from(t.as_str())),
            Operand::Paren(e) => self.eval(s, e),
            Operand::Var(name, idx) => {
                let i = idx.as_ref().map(|e| self.eval(s, e).int()).unwrap_or(0);
                match s.var(name) {
                    Some(v) => {
                        if i < 0 || i as usize >= v.cells.len() {
                            self.host.log(&s.name, &format!("Array index out of bounce: '{name}'"));
                            return Value::Int(0);
                        }
                        v.cells[i as usize].clone()
                    }
                    None => {
                        self.host.log(&s.name, &format!("Unknown symbol: '{name}'"));
                        Value::Int(0)
                    }
                }
            }
            Operand::Call(name, args) => {
                let vals: Vec<Value> = args.iter().map(|a| self.eval(s, a)).collect();
                self.call(s, name, vals)
            }
        }
    }

    fn call(&mut self, s: &mut Script, name: &str, args: Vec<Value>) -> Value {
        let a = |k: usize| args.get(k).cloned().unwrap_or_default();
        match name {
            "Sleep" => {
                let ms = a(0).int();
                let until = (ms != -1).then(|| self.sched_tick + (ms.max(0) as u32) / MS_PER_TICK);
                self.latent = Some(Pending::Sleep { until });
                return Value::Int(1);
            }
            "RunScript" | "RunScriptInt" | "RunScriptInt2" | "RunScriptString" => {
                self.fx.spawn.push((a(0).str().to_string(), args[1..].to_vec(), s.context));
                return Value::Int(1);
            }
            "StopScript" => {
                self.fx.stop.push(ScriptId(a(0).int() as u32));
                return Value::Int(0);
            }
            "FindScript" => {
                let id = self.running.iter().find(|(n, _)| n.eq_ignore_ascii_case(a(0).str())).map(|r| r.1 .0);
                return Value::Int(id.unwrap_or(0) as i32);
            }
            "KillLocalScripts" => {
                self.fx.kill_local = Some((s.context, s.id));
                return Value::Int(0);
            }
            _ => {}
        }
        let ctx = CallCtx { script: s.id, script_name: &s.name, poll: false, tick: self.sched_tick };
        match self.host.call(&ctx, name, &args) {
            HostResult::Done(v) => Value::Int(v),
            HostResult::Latent(v) => {
                self.latent = Some(Pending::Host { name: name.to_string(), args });
                Value::Int(v)
            }
            HostResult::Unknown => {
                self.host.log(&s.name, &format!("Unknown symbol: '{name}'"));
                Value::Int(0)
            }
        }
    }
}
