use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug, Clone)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
}

impl CancelToken {
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }
}

fn registry() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static REG: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn start_job(job_id: &str) -> CancelToken {
    let flag = Arc::new(AtomicBool::new(false));
    if let Ok(mut guard) = registry().lock() {
        if let Some(prev) = guard.get(job_id) {
            prev.store(true, Ordering::Relaxed);
        }
        guard.insert(job_id.to_string(), Arc::clone(&flag));
    }
    CancelToken { flag }
}

pub fn cancel_job(job_id: &str) -> bool {
    let Ok(guard) = registry().lock() else {
        return false;
    };
    if job_id.is_empty() || job_id == "*" || job_id == "__ALL__" {
        let mut any = false;
        for flag in guard.values() {
            flag.store(true, Ordering::Relaxed);
            any = true;
        }
        return any;
    }
    if let Some(flag) = guard.get(job_id) {
        flag.store(true, Ordering::Relaxed);
        return true;
    }
    false
}

pub fn finish_job(job_id: &str) {
    if let Ok(mut guard) = registry().lock() {
        guard.remove(job_id);
    }
}
