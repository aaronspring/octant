//! Pure-Rust WebAssembly symbols shim for zstd C API to satisfy `icechunk-format` linking.

#[cfg(target_arch = "wasm32")]
mod shim {
    use std::io::Read;

    #[repr(C)]
    pub struct ZstdDCtx {
        _unused: u8,
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ZSTD_isError(result: usize) -> core::ffi::c_uint {
        if result > (-(128isize)) as usize {
            1
        } else {
            0
        }
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ZSTD_getErrorName(_result: usize) -> *const core::ffi::c_char {
        c"Zstd error".as_ptr()
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ZSTD_createDCtx() -> *mut ZstdDCtx {
        Box::into_raw(Box::new(ZstdDCtx { _unused: 0 }))
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ZSTD_freeDCtx(dctx: *mut ZstdDCtx) -> usize {
        if !dctx.is_null() {
            unsafe {
                drop(Box::from_raw(dctx));
            }
        }
        0
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ZSTD_DCtx_loadDictionary(
        _dctx: *mut ZstdDCtx,
        _dict: *const core::ffi::c_void,
        _dict_size: usize,
    ) -> usize {
        0
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ZSTD_decompressDCtx(
        _dctx: *mut ZstdDCtx,
        dst: *mut core::ffi::c_void,
        dst_capacity: usize,
        src: *const core::ffi::c_void,
        src_size: usize,
    ) -> usize {
        if src.is_null() || dst.is_null() {
            return (-(1isize)) as usize;
        }
        let src_slice = unsafe { core::slice::from_raw_parts(src as *const u8, src_size) };
        let mut cursor = std::io::Cursor::new(src_slice);
        let mut decoder = match ruzstd::decoding::StreamingDecoder::new(&mut cursor) {
            Ok(d) => d,
            Err(_) => return (-(1isize)) as usize,
        };
        let dst_slice = unsafe { core::slice::from_raw_parts_mut(dst as *mut u8, dst_capacity) };
        let mut total = 0;
        while total < dst_capacity {
            match decoder.read(&mut dst_slice[total..]) {
                Ok(0) => break,
                Ok(n) => total += n,
                Err(_) => return (-(1isize)) as usize,
            }
        }
        total
    }
}
