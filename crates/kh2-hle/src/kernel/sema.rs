//! Semaphores. The parameter and status block is the SDK's `ee_sema_t` [SDK]: count 0x00, max
//! count 0x04, initial count 0x08, waiting threads 0x0c (the frame wait 0x14d1a8 tests it
//! through ReferSemaStatus), attr 0x10, option 0x14. Attribute bit 0 selects priority order for waiters
//! instead of first come first served.

pub struct Sema {
    pub id: u32,
    pub count: i32,
    pub max: i32,
    pub init: i32,
    pub attr: u32,
    pub option: u32,
    /// Waiting thread ids in arrival order.
    pub waiters: Vec<u32>,
}

impl Sema {
    pub const SA_THPRI: u32 = 1;
}
