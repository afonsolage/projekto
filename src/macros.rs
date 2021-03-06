#[allow(unused_macros)]
macro_rules! fn_name {
    () => {{
        fn f() {}
        fn type_name_of<T>(_: T) -> &'static str {
            std::any::type_name::<T>()
        }
        let name = type_name_of(f);

        // Find and cut the rest of the path
        match &name[..name.len() - 3].rfind(':') {
            Some(pos) => &name[pos + 1..name.len() - 3],
            None => &name[..name.len() - 3],
        }
    }};
}

#[allow(unused_macros)]
macro_rules! perf_fn {
    () => {{
        #[cfg(feature = "perf_counter")]
        {
            crate::debug::perf::PerfCounterGuard::new(fn_name!())
        }
        #[cfg(not(feature = "perf_counter"))]
        1
    }};
}

#[allow(unused_macros)]
macro_rules! perf_scope {
    ($var:ident) => {
        #[cfg(feature = "perf_counter")]
        let _perf = $var.measure();
    };
}

macro_rules! perf_fn_scope {
    () => {
        let mut _perf = perf_fn!();
        perf_scope!(_perf);
    };
}

macro_rules! trace_system_run {
    ($e:expr) => {
        trace!("{} working on {:?}", fn_name!(), $e);
    };
    () => {
        trace!("{}", fn_name!());
    };
}
