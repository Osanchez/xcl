//! The ray-tracing chain: which compute pipelines belong to it (made by the console BVH builder
//! module, or with a ray-intersection instruction, or listed by record key), and which of them are
//! left out at creation (a leave-out rule names them, or the chain is off). The roles live here,
//! keyed by the pointer the title holds for the pipeline.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use super::leave_out::{self, Role, Rules, Token};
use super::pso_async::LeftOut;
use crate::config::ChainMode;
use crate::gpu::effects::Effects;
use crate::pe::ExportTarget;

/// The console BVH builder's constructor (an SDK library export): the module that exports it is
/// the builder.
const BUILDER_CTOR: &str = "??0XboxBvhBuilder@@QEAA@PEAUID3D12Device11@@_N@Z";
/// Base of the module found to export `BUILDER_CTOR` (0: not found yet). Positive-only cache.
static BUILDER_BASE: AtomicUsize = AtomicUsize::new(0);

/// Whether a module with these exports is the builder.
fn exports_builder(by_name: &HashMap<String, ExportTarget>) -> bool {
    by_name.contains_key(BUILDER_CTOR)
}

/// The mapped image `addr` lies in: its base, and whether it is the builder. None for an address
/// in no mapped image (XCL's own code, the system's).
fn module_of(addr: usize) -> Option<(usize, bool)> {
    let known = BUILDER_BASE.load(Ordering::Acquire);
    if known != 0 {
        return crate::modules::image_base_for_addr(addr).map(|base| (base, base == known));
    }
    let m = crate::modules::find_mapped_by_addr(addr)?;
    let builder = exports_builder(&m.exports.by_name);
    if builder {
        BUILDER_BASE.store(m.base, Ordering::Release);
    }
    Some((m.base, builder))
}

/// The first frame that lies in a mapped image decides: Some(whether it is the builder), None when
/// no frame is in an image.
fn first_image_frame(frames: &[usize], module_of: &dyn Fn(usize) -> Option<(usize, bool)>) -> Option<bool> {
    frames.iter().find_map(|&a| module_of(a).map(|(_, builder)| builder))
}

/// Whether the call these return addresses are of was made by the builder: the first title frame
/// (the first one in a mapped image) is in the builder module.
fn classify_frames(frames: &[usize], module_of: &dyn Fn(usize) -> Option<(usize, bool)>) -> bool {
    first_image_frame(frames, module_of).unwrap_or(false)
}

/// Return addresses of this call, skipping this function's own frame.
#[inline(never)]
fn frames() -> ([usize; 16], usize) {
    use windows_sys::Win32::System::Diagnostics::Debug::RtlCaptureStackBackTrace;
    let mut f = [std::ptr::null_mut::<c_void>(); 16];
    let n = unsafe { RtlCaptureStackBackTrace(1, 16, f.as_mut_ptr(), std::ptr::null_mut()) } as usize;
    (std::array::from_fn(|i| f[i] as usize), n.min(16))
}

/// Whether the pipeline being created now is made by the builder module.
fn made_by_builder() -> bool {
    let (f, n) = frames();
    match first_image_frame(&f[..n], &module_of) {
        Some(builder) => builder,
        None => {
            static ONCE: AtomicBool = AtomicBool::new(false);
            if !ONCE.swap(true, Ordering::Relaxed) {
                crate::xlog_warn!("rt chain: no title frame for a compute pipeline");
            }
            false
        }
    }
}

/// What the user configured: the leave-out rules, the chain list and the chain's mode.
struct Settings {
    rules: Rules,
    chain: Vec<(Token, Role)>,
    mode: ChainMode,
}

static SETTINGS: OnceLock<Settings> = OnceLock::new();
/// indices of the chain entries a pipeline matched
static MATCHED: Mutex<Option<HashSet<usize>>> = Mutex::new(None);
static LEFT_BY_RULE: AtomicUsize = AtomicUsize::new(0);
static LEFT_BY_OFF: AtomicUsize = AtomicUsize::new(0);
/// pipelines that had a role
static ROLED: AtomicUsize = AtomicUsize::new(0);

/// The config's text, a newline, the environment variable's.
fn text_of(config: &str, var: &str) -> String {
    format!("{config}\n{}", std::env::var(var).unwrap_or_default())
}

/// Read once, at the first pipeline: `leave_out` / `XCL_LEAVE_OUT`, `raytracing_chain` /
/// `XCL_RT_CHAIN` (`@file` paths relative to the title's data folder), and the chain's mode.
fn settings() -> &'static Settings {
    SETTINGS.get_or_init(|| {
        let cfg = crate::state::config();
        let base = crate::state::data_dir();
        let (rules, w1) = leave_out::parse(&text_of(&cfg.leave_out, "XCL_LEAVE_OUT"), &base);
        let (chain, w2) = leave_out::parse_chain(&text_of(&cfg.raytracing_chain, "XCL_RT_CHAIN"), &base);
        for w in w1.iter().chain(&w2) {
            crate::xlog_warn!("rt chain: {w}");
        }
        Settings { rules: Rules::new(rules), chain, mode: cfg.raytracing_chain_mode() }
    })
}

/// The role of a pipeline, and whether (and why, by which rule's text) it is left out. The last
/// value is the index of the chain entry whose record key is `record`. The role is Build when the
/// builder made it, else Trace when its program traces, else the listed entry's role, else None.
/// It is left out when a rule names it, or when it has a role and the chain is off.
pub fn decide(
    record: u64,
    fx: Option<&Effects>,
    builder: bool,
    rules: &Rules,
    chain: &[(Token, Role)],
    mode: ChainMode,
) -> (Role, Option<(LeftOut, String)>, Option<usize>) {
    let listed = chain.iter().position(|(t, _)| matches!(t, Token::Record(k) if *k == record));
    let role = if builder {
        Role::Build
    } else if fx.is_some_and(|f| f.traces) {
        Role::Trace
    } else {
        listed.map_or(Role::None, |i| chain[i].1)
    };
    let left = match rules.matches(record, None, fx, role) {
        Some(text) => Some((LeftOut::Rule, text)),
        None if role != Role::None && mode == ChainMode::Off => Some((LeftOut::RtOff, "rt_off".to_string())),
        None => None,
    };
    (role, left, listed)
}

/// The role of the compute pipeline with console shader `cs` that is being created, and why it is
/// left out, if it is.
pub fn classify(cs: &[u8]) -> (Role, Option<LeftOut>) {
    let s = settings();
    let record = crate::gpu::shader::fnv64(cs);
    let fx = crate::gpu::effects::compute_facts(cs);
    let (role, left, listed) = decide(record, fx.as_ref(), made_by_builder(), &s.rules, &s.chain, s.mode);
    if let Some(i) = listed {
        MATCHED.lock().unwrap().get_or_insert_with(HashSet::new).insert(i);
    }
    if role != Role::None {
        ROLED.fetch_add(1, Ordering::Relaxed);
    }
    let Some((why, rule)) = left else { return (role, None) };
    match why {
        LeftOut::Rule => LEFT_BY_RULE.fetch_add(1, Ordering::Relaxed),
        LeftOut::RtOff => LEFT_BY_OFF.fetch_add(1, Ordering::Relaxed),
    };
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    if n < 64 || n.is_power_of_two() {
        crate::xlog_info!("rt chain: compute pipeline {record:016x} left out ({rule}), role {role:?} ({} so far)", n + 1);
    }
    crate::report::telemetry_left_out(record, &rule, "cs");
    (role, Some(why))
}

/// One line for the end of a run: None when nothing was configured and no pipeline had a role.
pub fn summary() -> Option<String> {
    let s = SETTINGS.get()?;
    let roled = ROLED.load(Ordering::Relaxed);
    if s.rules.is_empty() && s.chain.is_empty() && roled == 0 {
        return None;
    }
    let matched = MATCHED.lock().unwrap().as_ref().map_or(0, |m| m.len());
    let (by_rule, by_off) = (LEFT_BY_RULE.load(Ordering::Relaxed), LEFT_BY_OFF.load(Ordering::Relaxed));
    Some(format!(
        "rt chain: {}; list {} entries, {matched} matched a pipeline; left out {} pipelines ({by_rule} by rule, {by_off} by raytracing off)",
        if s.mode == ChainMode::On { "on" } else { "off" },
        s.chain.len(),
        by_rule + by_off,
    ))
}

/// title-visible pipeline pointer -> its chain role and whether its real pipeline exists
static ROLES: Mutex<Option<HashMap<usize, (Role, bool)>>> = Mutex::new(None);
/// fast path: no map lookup while false
static ANY_ROLE: AtomicBool = AtomicBool::new(false);

/// Pipeline `pso` (the pointer the title holds) has `role`; `built`: its real pipeline exists.
/// Role::None is not kept.
pub fn note_role(pso: usize, role: Role, built: bool) {
    if role == Role::None {
        return;
    }
    ANY_ROLE.store(true, Ordering::Release);
    ROLES.lock().unwrap().get_or_insert_with(HashMap::new).insert(pso, (role, built));
}

/// The role of the pipeline `pso` (the pointer a bind holds); Role::None when it has none.
pub fn role_of(pso: usize) -> Role {
    if !ANY_ROLE.load(Ordering::Acquire) {
        return Role::None;
    }
    ROLES.lock().unwrap().as_ref().and_then(|m| m.get(&pso)).map_or(Role::None, |r| r.0)
}

/// The title released `pso`: its address may come back for another object.
pub fn forget(pso: usize) {
    if !ANY_ROLE.load(Ordering::Acquire) {
        return;
    }
    if let Some(m) = ROLES.lock().unwrap().as_mut() {
        m.remove(&pso);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const KEY: u64 = 0x0123456789abcdef;

    fn exports(names: &[&str]) -> HashMap<String, ExportTarget> {
        names.iter().enumerate().map(|(i, n)| (n.to_string(), ExportTarget::Rva(i as u32))).collect()
    }

    fn rules(text: &str) -> Rules {
        let (t, w) = leave_out::parse(text, std::path::Path::new("."));
        assert!(w.is_empty(), "{w:?}");
        Rules::new(t)
    }

    #[test]
    fn exports_builder_finds_the_constructor() {
        assert!(exports_builder(&exports(&["SomethingElse", BUILDER_CTOR])));
        assert!(!exports_builder(&exports(&["SomethingElse", "??1XboxBvhBuilder@@QEAA@XZ"])));
        assert!(!exports_builder(&HashMap::new()));
    }

    // (module 0x1000: the title's exe, 0x2000: the builder, 0x3000: a system module, in no image)
    fn modules(addr: usize) -> Option<(usize, bool)> {
        match addr {
            0x1000..=0x1fff => Some((0x1000, false)),
            0x2000..=0x2fff => Some((0x2000, true)),
            _ => None,
        }
    }

    #[test]
    fn classify_frames_first_title_frame_in_builder() {
        assert!(classify_frames(&[0x2100, 0x1100], &modules));
        assert!(!classify_frames(&[0x1100, 0x2100], &modules), "the exe is first: a builder frame deeper does not count");
    }

    #[test]
    fn classify_frames_skips_non_image_frames() {
        assert!(classify_frames(&[0x9000, 0x9100, 0x2100, 0x1100], &modules));
        assert!(!classify_frames(&[0x9000, 0x1100, 0x2100], &modules));
    }

    #[test]
    fn classify_frames_none_without_title_frame() {
        assert!(!classify_frames(&[0x9000, 0x9100], &modules));
        assert!(!classify_frames(&[], &modules));
        assert_eq!(first_image_frame(&[0x9000], &modules), None);
    }

    /// The builder module may be mapped after the first pipeline: nothing is cached for a miss.
    #[test]
    fn builder_found_after_late_load() {
        let loaded = Cell::new(false);
        let module_of = |addr: usize| match addr {
            0x2000..=0x2fff if loaded.get() => Some((0x2000, true)),
            0x2000..=0x2fff => Some((0x2000, false)),
            _ => None,
        };
        let frames = [0x9000, 0x2100, 0x1100];
        assert!(!classify_frames(&frames, &module_of), "not loaded yet: not the builder");
        loaded.set(true);
        assert!(classify_frames(&frames, &module_of), "loaded: the builder");
    }

    #[test]
    fn rt_chain_classifies_trace_by_effects() {
        let traces = Effects { traces: true, ..Effects::default() };
        let plain = Effects::default();
        let none = Rules::default();
        let (role, left, listed) = decide(KEY, Some(&traces), false, &none, &[], ChainMode::On);
        assert_eq!((role, left, listed), (Role::Trace, None, None));
        // the builder wins over the program's own instruction
        assert_eq!(decide(KEY, Some(&traces), true, &none, &[], ChainMode::On).0, Role::Build);
        assert_eq!(decide(KEY, Some(&plain), true, &none, &[], ChainMode::On).0, Role::Build);
        // no facts, not the builder, not listed: no role, and never left out by the chain
        assert_eq!(decide(KEY, None, false, &none, &[], ChainMode::Off), (Role::None, None, None));
        assert_eq!(decide(KEY, Some(&plain), false, &none, &[], ChainMode::Off), (Role::None, None, None));
    }

    #[test]
    fn rt_chain_list_marks_consumer_by_record() {
        let chain = vec![
            (Token::Name("CS_TestResolve".into()), Role::Producer),
            (Token::Record(KEY + 1), Role::Producer),
            (Token::Record(KEY), Role::Consumer),
        ];
        let none = Rules::default();
        let (role, left, listed) = decide(KEY, None, false, &none, &chain, ChainMode::On);
        assert_eq!((role, left, listed), (Role::Consumer, None, Some(2)));
        assert_eq!(decide(KEY + 1, None, false, &none, &chain, ChainMode::On).0, Role::Producer);
        // a name entry matches no record key
        assert_eq!(decide(KEY + 2, None, false, &none, &chain, ChainMode::On), (Role::None, None, None));
        // a listed pipeline of the chain is left out when the chain is off
        let (role, left, listed) = decide(KEY, None, false, &none, &chain, ChainMode::Off);
        assert_eq!((role, left, listed), (Role::Consumer, Some((LeftOut::RtOff, "rt_off".to_string())), Some(2)));
        // the role of a builder or a trace stays when the key is listed too
        assert_eq!(decide(KEY, None, true, &none, &chain, ChainMode::On), (Role::Build, None, Some(2)));
    }

    #[test]
    fn rt_off_leaves_out_every_role_and_keeps_none() {
        let none = Rules::default();
        let traces = Effects { traces: true, ..Effects::default() };
        let chain = vec![(Token::Record(KEY), Role::Producer)];
        for (fx, builder, key) in [(None, true, 1u64), (Some(&traces), false, 2), (None, false, KEY)] {
            let (role, left, _) = decide(key, fx, builder, &none, &chain, ChainMode::Off);
            assert_ne!(role, Role::None);
            assert_eq!(left, Some((LeftOut::RtOff, "rt_off".to_string())), "{role:?}");
            assert_eq!(decide(key, fx, builder, &none, &chain, ChainMode::On).1, None);
        }
        // no role: kept, whatever the mode
        assert_eq!(decide(3, Some(&Effects::default()), false, &none, &chain, ChainMode::Off).1, None);
    }

    #[test]
    fn leave_out_rule_wins_over_chain_on() {
        let chain = vec![(Token::Record(KEY), Role::Consumer)];
        let r = rules("0123456789abcdef");
        // the chain is on and the program has a role: the rule still leaves it out, by its text
        let (role, left, _) = decide(KEY, None, false, &r, &chain, ChainMode::On);
        assert_eq!((role, left), (Role::Consumer, Some((LeftOut::Rule, "0123456789abcdef".to_string()))));
        // with the chain off the rule is named, not the chain
        assert_eq!(decide(KEY, None, false, &r, &chain, ChainMode::Off).1, Some((LeftOut::Rule, "0123456789abcdef".to_string())));
        // a program with no role at all is left out by a rule too
        assert_eq!(decide(KEY, None, false, &r, &[], ChainMode::On), (Role::None, Some((LeftOut::Rule, "0123456789abcdef".to_string())), None));
        // a class rule reads the role; `!` wins
        let r = rules("class:build");
        assert_eq!(decide(1, None, true, &r, &[], ChainMode::On).1, Some((LeftOut::Rule, "class:build".to_string())));
        let r = rules("class:build !0123456789abcdef");
        assert_eq!(decide(KEY, None, true, &r, &[], ChainMode::On).1, None);
        assert_eq!(decide(KEY, None, true, &r, &[], ChainMode::Off).1, Some((LeftOut::RtOff, "rt_off".to_string())));
    }

    // (only the made-up pointers 0x7E57_E000..=0x7E57_E0FF here: the maps are process-wide)
    #[test]
    fn roles_keyed_by_title_pointer_warm_and_standin() {
        let (warm, standin, none) = (0x7E57_E001usize, 0x7E57_E002usize, 0x7E57_E003usize);
        note_role(warm, Role::Build, true);
        note_role(standin, Role::Trace, false);
        note_role(none, Role::None, true);
        assert_eq!(role_of(warm), Role::Build);
        assert_eq!(role_of(standin), Role::Trace);
        assert_eq!(role_of(none), Role::None, "Role::None is not kept");
        assert_eq!(ROLES.lock().unwrap().as_ref().unwrap().get(&warm), Some(&(Role::Build, true)));
        assert_eq!(ROLES.lock().unwrap().as_ref().unwrap().get(&standin), Some(&(Role::Trace, false)));
        assert!(!ROLES.lock().unwrap().as_ref().unwrap().contains_key(&none));
        // a later creation at the same pointer replaces it
        note_role(warm, Role::Consumer, false);
        assert_eq!(role_of(warm), Role::Consumer);
        forget(warm);
        forget(standin);
    }

    #[test]
    fn forget_drops_role_so_a_reused_pointer_has_none() {
        let pso = 0x7E57_E011usize;
        note_role(pso, Role::Producer, false);
        assert_eq!(role_of(pso), Role::Producer);
        forget(pso);
        assert_eq!(role_of(pso), Role::None);
        forget(pso);
        forget(0x7E57_E012);
        assert_eq!(role_of(pso), Role::None);
    }

    #[test]
    fn role_of_is_none_without_any_role() {
        // (another test may have set a role: a pointer nobody noted has none either way)
        assert_eq!(role_of(0x7E57_E0F0), Role::None);
        assert_eq!(role_of(0), Role::None);
        forget(0x7E57_E0F0);
        assert_eq!(role_of(0x7E57_E0F0), Role::None);
    }
}
