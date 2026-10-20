//! Test-only observation of actual GLES bitmap transfers made by Ganesh.

use std::{cell::Cell, ffi::c_void};

type TexImage = unsafe extern "system" fn(u32, i32, i32, i32, i32, i32, u32, u32, *const c_void);
type TexSubImage = unsafe extern "system" fn(u32, i32, i32, i32, i32, i32, u32, u32, *const c_void);

thread_local! {
    static IMAGE: Cell<Option<TexImage>> = const { Cell::new(None) };
    static SUB_IMAGE: Cell<Option<TexSubImage>> = const { Cell::new(None) };
    static UPLOADS: Cell<u64> = const { Cell::new(0) };
}

pub(super) fn count() -> u64 {
    UPLOADS.with(Cell::get)
}

pub(super) fn load(name: &str, pointer: *const c_void) -> *const c_void {
    if pointer.is_null() {
        return pointer;
    }
    // SAFETY: the loader supplies the named GLES entry point. Wrappers retain
    // its exact ABI and are used only on the context-owning thread.
    match name {
        "glTexImage2D" => {
            IMAGE.with(|original| {
                original.set(Some(unsafe {
                    std::mem::transmute::<*const c_void, TexImage>(pointer)
                }))
            });
            image as *const c_void
        }
        "glTexSubImage2D" => {
            SUB_IMAGE.with(|original| {
                original.set(Some(unsafe {
                    std::mem::transmute::<*const c_void, TexSubImage>(pointer)
                }))
            });
            sub_image as *const c_void
        }
        _ => pointer,
    }
}

unsafe extern "system" fn image(
    target: u32,
    level: i32,
    internal: i32,
    width: i32,
    height: i32,
    border: i32,
    format: u32,
    kind: u32,
    pixels: *const c_void,
) {
    if !pixels.is_null() {
        UPLOADS.with(|count| count.set(count.get() + 1));
    }
    IMAGE.with(|original| {
        // SAFETY: `load` installs the original before exposing this wrapper.
        unsafe {
            original.get().unwrap()(
                target, level, internal, width, height, border, format, kind, pixels,
            )
        };
    });
}

unsafe extern "system" fn sub_image(
    target: u32,
    level: i32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    format: u32,
    kind: u32,
    pixels: *const c_void,
) {
    UPLOADS.with(|count| count.set(count.get() + 1));
    SUB_IMAGE.with(|original| {
        // SAFETY: `load` installs the original before exposing this wrapper.
        unsafe {
            original.get().unwrap()(target, level, x, y, width, height, format, kind, pixels)
        };
    });
}
