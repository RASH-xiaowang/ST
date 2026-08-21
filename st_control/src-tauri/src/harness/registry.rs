// ============================================================
// Harness — Cordis-lite 服务注册表（DSH「一切皆插件」地基）
//
// DSH 在 Cordis 上以插件提供服务与事件，注册即效应、卸载可逆。
// 本模块是纯原生迁移的最小等价物：
// - provide(name, Arc<T>) → Disposer（Drop 时自动移除注册，效应可逆）
// - get::<T>(name) → Option<Arc<T>>（按类型下转型）
// - remove(name) 显式移除并返还服务
// 服务以 &'static str 命名（对应 Cordis 的 ctx 键）。
// ============================================================

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

type BoxedService = Arc<dyn Any + Send + Sync>;

/// 注册项：服务本体 + 类型 id（get 时按类型精确下转型）
struct Entry {
    type_id: TypeId,
    service: BoxedService,
}

fn registry() -> &'static Mutex<HashMap<&'static str, Entry>> {
    static REG: OnceLock<Mutex<HashMap<&'static str, Entry>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 注册一个服务；返回的 Disposer 在 Drop 时撤销注册（效应可逆）
pub fn provide<T: Any + Send + Sync>(name: &'static str, service: Arc<T>) -> Disposer {
    registry().lock().unwrap().insert(
        name,
        Entry {
            type_id: TypeId::of::<T>(),
            service,
        },
    );
    Disposer { name, armed: true }
}

/// 按名称与类型获取服务；类型不符视为未注册
pub fn get<T: Any + Send + Sync>(name: &'static str) -> Option<Arc<T>> {
    let m = registry().lock().unwrap();
    let entry = m.get(name)?;
    if entry.type_id != TypeId::of::<T>() {
        return None;
    }
    entry.service.clone().downcast::<T>().ok()
}

/// 显式移除并返还服务（配合 Disposer::disarm 使用）。
/// 后续阶段（preset 热重载、isolate realm）会使用；先声明 API 面。
#[allow(dead_code)]
pub fn remove(name: &'static str) -> Option<BoxedService> {
    registry().lock().unwrap().remove(name).map(|e| e.service)
}

/// 注册撤销器：Drop 时移除注册；disarm() 后放弃所有权
pub struct Disposer {
    name: &'static str,
    armed: bool,
}

impl Disposer {
    /// 放弃撤销（服务保持注册，由注册表持有）
    pub fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for Disposer {
    fn drop(&mut self) {
        if self.armed {
            registry().lock().unwrap().remove(self.name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provide_get_and_reversible_dispose() {
        let s = Arc::new(String::from("hello"));
        let d = provide("test.str", s);
        assert_eq!(
            get::<String>("test.str").map(|v| v.as_str() == "hello"),
            Some(true)
        );
        // 类型不符 → None
        assert!(get::<u32>("test.str").is_none());
        drop(d);
        assert!(get::<String>("test.str").is_none());
    }

    #[test]
    fn disarm_keeps_service() {
        let d = provide("test.keep", Arc::new(42u32));
        d.disarm();
        assert_eq!(get::<u32>("test.keep").map(|v| *v), Some(42));
        let _ = remove("test.keep");
        assert!(get::<u32>("test.keep").is_none());
    }
}
