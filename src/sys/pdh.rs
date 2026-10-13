//! Minimal wrapper over Windows performance counters (PDH).

use windows::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA,
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhGetFormattedCounterValue, PdhOpenQueryW,
};
use windows::core::{HSTRING, PCWSTR};

pub struct Query(PDH_HQUERY);

#[derive(Clone, Copy)]
pub struct Counter(PDH_HCOUNTER);

impl Query {
    pub fn new() -> Option<Self> {
        let mut q = PDH_HQUERY::default();
        // SAFETY: out-pointer to a local; null data source = live data.
        let status = unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut q) };
        (status == 0).then_some(Self(q))
    }

    /// Adds a counter by its English path, e.g. `\Processor Information(_Total)\% Processor Utility`.
    pub fn add(&self, path: &str) -> Option<Counter> {
        let mut c = PDH_HCOUNTER::default();
        // SAFETY: valid query handle; HSTRING outlives the call.
        let status = unsafe { PdhAddEnglishCounterW(self.0, &HSTRING::from(path), 0, &mut c) };
        (status == 0).then_some(Counter(c))
    }

    /// Takes a sample. Rate counters need two samples before they have a value.
    pub fn collect(&self) {
        // SAFETY: valid query handle.
        unsafe { PdhCollectQueryData(self.0) };
    }
}

impl Drop for Query {
    fn drop(&mut self) {
        // SAFETY: handle came from PdhOpenQueryW and is closed once.
        unsafe { PdhCloseQuery(self.0) };
    }
}

impl Counter {
    pub fn value(self) -> Option<f64> {
        let mut v = PDH_FMT_COUNTERVALUE::default();
        // SAFETY: valid counter handle; out-pointer to a local.
        let status = unsafe { PdhGetFormattedCounterValue(self.0, PDH_FMT_DOUBLE, None, &mut v) };
        // SAFETY: PDH_FMT_DOUBLE selects the doubleValue member.
        (status == 0 && v.CStatus == 0).then_some(unsafe { v.Anonymous.doubleValue })
    }

    /// All instances of a wildcard counter, e.g. `\GPU Engine(*)\Utilization Percentage`.
    pub fn array(self) -> Vec<(String, f64)> {
        let (mut size, mut count) = (0u32, 0u32);
        // SAFETY: first call only queries the buffer size.
        let status = unsafe { PdhGetFormattedCounterArrayW(self.0, PDH_FMT_DOUBLE, &mut size, &mut count, None) };
        if status != PDH_MORE_DATA || size == 0 {
            return Vec::new();
        }
        // The buffer holds the item array followed by the instance name strings.
        let item = size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
        let mut buf: Vec<PDH_FMT_COUNTERVALUE_ITEM_W> = Vec::with_capacity((size as usize).div_ceil(item));
        // SAFETY: buffer is at least `size` bytes; PDH fills `count` items.
        let status = unsafe {
            PdhGetFormattedCounterArrayW(self.0, PDH_FMT_DOUBLE, &mut size, &mut count, Some(buf.as_mut_ptr()))
        };
        if status != 0 {
            return Vec::new();
        }
        // SAFETY: PDH initialised `count` items.
        unsafe { buf.set_len(count as usize) };
        buf.iter()
            .filter(|it| it.FmtValue.CStatus == 0)
            .map(|it| {
                // SAFETY: szName points into `buf`, which is still alive.
                let name = unsafe { it.szName.to_string() }.unwrap_or_default();
                // SAFETY: PDH_FMT_DOUBLE selects the doubleValue member.
                (name, unsafe { it.FmtValue.Anonymous.doubleValue })
            })
            .collect()
    }
}
