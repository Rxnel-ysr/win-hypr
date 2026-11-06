use crate::index::AppCache;
use std::rc::Rc;

pub type HideFn = Rc<dyn Fn()>;

/// Shared handles passed to every UI module.
#[derive(Clone)]
pub struct Ctx {
    pub hide: HideFn,
    pub cache: Rc<AppCache>,
}
