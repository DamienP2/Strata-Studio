//! Studio's own NDI routing, without a router: for each destination
//! (« GO1 », « Écran studio »…), Studio publishes an NDI routing source
//! (« STUDIO (GO1) ») and points it at the source chosen. The receivers
//! take Studio's routing source once and for all.
//!
//! It uses the NDI runtime (`libndi.so.6` or `.5`), loaded when Studio
//! starts if it is installed (the installer offers it): NDI's licence does
//! not let Studio carry it. The functions used are those of the NDI SDK:
//! `NDIlib_initialize`, `NDIlib_routing_create`, `NDIlib_routing_change`,
//! `NDIlib_routing_destroy`, and the finder to list the sources
//! (`NDIlib_find_create_v2`, `NDIlib_find_wait_for_sources`,
//! `NDIlib_find_get_current_sources`).

use crate::{DeviceError, Result, Router};
use std::collections::BTreeMap;
use std::ffi::{c_char, c_void, CStr, CString};

#[repr(C)]
struct Source {
    name: *const c_char,
    url: *const c_char,
}

#[repr(C)]
struct RoutingCreate {
    name: *const c_char,
    groups: *const c_char,
}

#[repr(C)]
struct FindCreate {
    show_local_sources: bool,
    groups: *const c_char,
    extra_ips: *const c_char,
}

type Initialize = unsafe extern "C" fn() -> bool;
type RoutingCreateFn = unsafe extern "C" fn(*const RoutingCreate) -> *mut c_void;
type RoutingChange = unsafe extern "C" fn(*mut c_void, *const Source) -> bool;
type RoutingDestroy = unsafe extern "C" fn(*mut c_void);
type FindCreateFn = unsafe extern "C" fn(*const FindCreate) -> *mut c_void;
type FindWait = unsafe extern "C" fn(*mut c_void, u32) -> bool;
type FindSources = unsafe extern "C" fn(*mut c_void, *mut u32) -> *const Source;
type FindDestroy = unsafe extern "C" fn(*mut c_void);

/// The NDI runtime's names, newest first.
pub const LIBRARIES: [&str; 3] = ["libndi.so.6", "libndi.so.5", "libndi.so"];

struct Instance {
    handle: *mut c_void,
    source: String,
    /// Kept alive while NDI may read it.
    _name: CString,
}

pub struct NdiRouter {
    lib: libloading::Library,
    prefix: String,
    routes: BTreeMap<String, Instance>,
    finder: *mut c_void,
}

// The NDI SDK's instances may be used from any thread, one at a time;
// Studio keeps the router behind a lock.
unsafe impl Send for NdiRouter {}

/// The routing source's name for a destination.
pub fn routing_name(prefix: &str, destination: &str) -> String {
    format!("{prefix} ({destination})")
}

impl NdiRouter {
    /// Loads the NDI runtime. `prefix`: the name the routing sources start
    /// with (« STUDIO »).
    pub fn load(prefix: &str) -> Result<Self> {
        let refused = |e: String| DeviceError::Refused("NDI".into(), e);
        let mut last = String::new();
        let lib = LIBRARIES
            .iter()
            .find_map(|name| match unsafe { libloading::Library::new(name) } {
                Ok(l) => Some(l),
                Err(e) => {
                    last = e.to_string();
                    None
                }
            })
            .ok_or_else(|| refused(format!("runtime NDI absent ({last})")))?;
        unsafe {
            let init: libloading::Symbol<Initialize> = lib
                .get(b"NDIlib_initialize\0")
                .map_err(|e| refused(e.to_string()))?;
            if !init() {
                return Err(refused("processeur non pris en charge par NDI".into()));
            }
            let find_create: libloading::Symbol<FindCreateFn> = lib
                .get(b"NDIlib_find_create_v2\0")
                .map_err(|e| refused(e.to_string()))?;
            let settings = FindCreate {
                show_local_sources: true,
                groups: std::ptr::null(),
                extra_ips: std::ptr::null(),
            };
            let finder = find_create(&settings);
            Ok(NdiRouter {
                lib,
                prefix: prefix.into(),
                routes: BTreeMap::new(),
                finder,
            })
        }
    }

    /// The NDI sources seen on the network now.
    pub fn sources(&self) -> Vec<String> {
        if self.finder.is_null() {
            return vec![];
        }
        unsafe {
            let (Ok(wait), Ok(get)) = (
                self.lib.get::<FindWait>(b"NDIlib_find_wait_for_sources\0"),
                self.lib
                    .get::<FindSources>(b"NDIlib_find_get_current_sources\0"),
            ) else {
                return vec![];
            };
            wait(self.finder, 200);
            let mut n = 0u32;
            let list = get(self.finder, &mut n);
            (0..n as usize)
                .filter_map(|i| {
                    let s = list.add(i);
                    (!(*s).name.is_null())
                        .then(|| CStr::from_ptr((*s).name).to_string_lossy().into_owned())
                })
                .collect()
        }
    }
}

impl Router for NdiRouter {
    fn name(&self) -> String {
        format!("routage NDI ({})", self.prefix)
    }
    fn route(&mut self, destination: &str, source: &str) -> Result<()> {
        let refused = |e: String| DeviceError::Refused("NDI".into(), e);
        let src_name = CString::new(source).map_err(|e| refused(e.to_string()))?;
        unsafe {
            if !self.routes.contains_key(destination) {
                let create: libloading::Symbol<RoutingCreateFn> = self
                    .lib
                    .get(b"NDIlib_routing_create\0")
                    .map_err(|e| refused(e.to_string()))?;
                let name = CString::new(routing_name(&self.prefix, destination))
                    .map_err(|e| refused(e.to_string()))?;
                let settings = RoutingCreate {
                    name: name.as_ptr(),
                    groups: std::ptr::null(),
                };
                let handle = create(&settings);
                if handle.is_null() {
                    return Err(refused(format!("routage « {destination} » non créé")));
                }
                self.routes.insert(
                    destination.to_string(),
                    Instance {
                        handle,
                        source: String::new(),
                        _name: name,
                    },
                );
            }
            let change: libloading::Symbol<RoutingChange> = self
                .lib
                .get(b"NDIlib_routing_change\0")
                .map_err(|e| refused(e.to_string()))?;
            let inst = self.routes.get_mut(destination).expect("created above");
            let s = Source {
                name: src_name.as_ptr(),
                url: std::ptr::null(),
            };
            if !change(inst.handle, &s) {
                return Err(refused(format!("{destination} ← {source} refusé")));
            }
            inst.source = source.to_string();
        }
        Ok(())
    }
    fn routes(&self) -> Vec<(String, String)> {
        self.routes
            .iter()
            .map(|(d, i)| (d.clone(), i.source.clone()))
            .collect()
    }
}

impl Drop for NdiRouter {
    fn drop(&mut self) {
        unsafe {
            if let Ok(destroy) = self.lib.get::<RoutingDestroy>(b"NDIlib_routing_destroy\0") {
                for i in self.routes.values() {
                    destroy(i.handle);
                }
            }
            if !self.finder.is_null() {
                if let Ok(destroy) = self.lib.get::<FindDestroy>(b"NDIlib_find_destroy\0") {
                    destroy(self.finder);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(routing_name("STUDIO", "GO1"), "STUDIO (GO1)");
    }

    #[test]
    fn without_the_runtime_it_says_so() {
        // The test machines have no NDI runtime.
        if let Err(e) = NdiRouter::load("STUDIO") {
            assert!(e.to_string().contains("NDI"));
        }
    }
}
