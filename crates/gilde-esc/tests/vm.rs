use gilde_esc::vm::{CallCtx, Host, HostResult, Scheduler, Value};
use std::collections::HashMap;

#[derive(Default)]
struct Mock {
    sources: HashMap<String, String>,
    calls: Vec<(u32, String, Vec<i32>)>,
    /// name -> number of polls before completion
    latent: HashMap<String, u32>,
    polls: HashMap<String, u32>,
}

impl Host for Mock {
    fn call(&mut self, ctx: &CallCtx, name: &str, args: &[Value]) -> HostResult {
        if !ctx.poll {
            self.calls.push((ctx.tick, name.to_string(), args.iter().map(Value::int).collect()));
        }
        if let Some(&n) = self.latent.get(name) {
            let p = self.polls.entry(name.to_string()).or_default();
            if ctx.poll {
                *p += 1;
            }
            return if *p >= n { HostResult::Done(7) } else { HostResult::Latent(7) };
        }
        HostResult::Done(args.first().map(Value::int).unwrap_or(0))
    }
    fn load_source(&mut self, path: &str) -> Option<String> {
        self.sources.get(path).cloned()
    }
}

fn run(src: &str, ticks: u32) -> (Mock, Scheduler) {
    let mut host = Mock::default();
    host.sources.insert("t.esc".into(), src.into());
    host.latent.insert("Walk".into(), 3);
    let mut s = Scheduler::new();
    let id = s.run(&mut host, "t.esc", vec![], 0).unwrap();
    let _ = id;
    for _ in 0..ticks {
        s.tick(&mut host);
    }
    (host, s)
}

fn printed(h: &Mock) -> Vec<i32> {
    h.calls.iter().filter(|c| c.1 == "PrintInt").map(|c| c.2[0]).collect()
}

#[test]
fn no_operator_precedence() {
    let (h, _) = run("void main(void){ #multistep PrintInt(1+2*3); PrintInt(10-2-3); PrintInt(-5+1); }", 2);
    assert_eq!(printed(&h), vec![9, 5, -4]);
}

#[test]
fn comparison_takes_rest_and_logic_is_bitwise() {
    let (h, _) = run(
        "int a; void main(void){ #multistep a=2; PrintInt(a == 1 + 1); PrintInt(a+1 == 3); PrintInt(a==2 && a!=3); PrintInt(a==1 || a==2); }",
        2,
    );
    assert_eq!(printed(&h), vec![1, 1, 1, 1]);
}

#[test]
fn assignment_inside_expression_does_not_assign() {
    let (h, _) = run("int x; void main(void){ #multistep x=5; if((x=1)==1) PrintInt(x); }", 2);
    assert_eq!(printed(&h), vec![5]);
}

#[test]
fn loops_arrays_and_functions() {
    let (h, _) = run(
        "int v[3]; int i; void f(int k){ PrintInt(k*2); } void main(void){ #multistep i=0; while(i<3){ v[i]=i+10; f(v[i]); i++; } }",
        2,
    );
    assert_eq!(printed(&h), vec![20, 22, 24]);
}

#[test]
fn normal_mode_is_one_statement_per_tick() {
    let (h, _) = run("void main(void){ PrintInt(1); PrintInt(2); PrintInt(3); }", 2);
    assert_eq!(printed(&h), vec![1, 2]);
}

#[test]
fn sleep_waits_ms_over_14_ticks() {
    let (h, _) = run("void main(void){ #multistep PrintInt(1); Sleep(140); PrintInt(2); }", 30);
    let t: Vec<u32> = h.calls.iter().filter(|c| c.1 == "PrintInt").map(|c| c.0).collect();
    assert_eq!(t.len(), 2);
    assert_eq!(t[1] - t[0], 10);
}

#[test]
fn latent_calls_block_only_in_single_step() {
    let (h, _) = run("void main(void){ #singlestep Walk(1); PrintInt(1); }", 3);
    assert!(printed(&h).is_empty(), "single step must wait for Walk");
    let (h, _) = run("void main(void){ Walk(1); PrintInt(1); }", 3);
    assert_eq!(printed(&h), vec![1]);
}

#[test]
fn exit_handler_runs_after_main() {
    let (h, s) = run("void main(void){ PrintInt(1); } void exit(void){ PrintInt(99); }", 5);
    assert_eq!(printed(&h), vec![1, 99]);
    assert!(s.scripts().is_empty());
}

#[test]
fn byte_wraps_and_strings_pass_through() {
    let mut host = Mock::default();
    host.sources.insert(
        "t.esc".into(),
        "byte b; string s[2]; void main(string n){ #multistep b=127; b++; PrintInt(b); s[1]=n; Print(s[1]); }".into(),
    );
    let mut s = Scheduler::new();
    s.run(&mut host, "t.esc", vec![Value::Str("hans".into())], 0).unwrap();
    s.tick(&mut host);
    assert_eq!(printed(&host), vec![-128]);
}
