//! Pure-Rust WebAssembly symbols shim for zstd C API to satisfy `icechunk-format` linking
//! when compiling on toolchains without a C compiler for `wasm32-unknown-unknown`.

#[cfg(target_arch = "wasm32")]
mod shim {
    use std::io::Read;

    pub struct ZstdDCtx {
        decoder: Option<
            ruzstd::decoding::StreamingDecoder<
                std::io::Cursor<Vec<u8>>,
                ruzstd::decoding::FrameDecoder,
            >,
        >,
        input_accum: Vec<u8>,
    }

    #[repr(C)]
    pub struct ZstdInBuffer {
        pub src: *const core::ffi::c_void,
        pub size: usize,
        pub pos: usize,
    }

    #[repr(C)]
    pub struct ZstdOutBuffer {
        pub dst: *mut core::ffi::c_void,
        pub size: usize,
        pub pos: usize,
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
        Box::into_raw(Box::new(ZstdDCtx {
            decoder: None,
            input_accum: Vec::new(),
        }))
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
    pub extern "C" fn ZSTD_createDStream() -> *mut ZstdDCtx {
        ZSTD_createDCtx()
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ZSTD_freeDStream(zds: *mut ZstdDCtx) -> usize {
        ZSTD_freeDCtx(zds)
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ZSTD_initDStream(zds: *mut ZstdDCtx) -> usize {
        if !zds.is_null() {
            let ctx = unsafe { &mut *zds };
            ctx.decoder = None;
            ctx.input_accum.clear();
        }
        0
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ZSTD_DCtx_reset(zds: *mut ZstdDCtx, _reset: u32) -> usize {
        ZSTD_initDStream(zds)
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ZSTD_DStreamInSize() -> usize {
        131072
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ZSTD_DStreamOutSize() -> usize {
        131072
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
    pub extern "C" fn ZSTD_decompressStream(
        zds: *mut ZstdDCtx,
        output: *mut ZstdOutBuffer,
        input: *mut ZstdInBuffer,
    ) -> usize {
        if zds.is_null() || output.is_null() || input.is_null() {
            return (-(1isize)) as usize;
        }

        let ctx = unsafe { &mut *zds };
        let out_buf = unsafe { &mut *output };
        let in_buf = unsafe { &mut *input };

        if in_buf.pos < in_buf.size && !in_buf.src.is_null() {
            let chunk_len = in_buf.size - in_buf.pos;
            let src_bytes = unsafe {
                core::slice::from_raw_parts((in_buf.src as *const u8).add(in_buf.pos), chunk_len)
            };
            ctx.input_accum.extend_from_slice(src_bytes);
            in_buf.pos = in_buf.size;
        }

        if ctx.decoder.is_none() {
            let cursor = std::io::Cursor::new(std::mem::take(&mut ctx.input_accum));
            match ruzstd::decoding::StreamingDecoder::new(cursor) {
                Ok(dec) => {
                    ctx.decoder = Some(dec);
                }
                Err(_) => return (-(1isize)) as usize,
            }
        }

        let Some(ref mut dec) = ctx.decoder else {
            return (-(1isize)) as usize;
        };

        if out_buf.pos >= out_buf.size || out_buf.dst.is_null() {
            return 0;
        }

        let avail_out = out_buf.size - out_buf.pos;
        let dst_slice = unsafe {
            core::slice::from_raw_parts_mut((out_buf.dst as *mut u8).add(out_buf.pos), avail_out)
        };

        match dec.read(dst_slice) {
            Ok(0) => 0,
            Ok(n) => {
                out_buf.pos += n;
                0
            }
            Err(_) => (-(1isize)) as usize,
        }
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
