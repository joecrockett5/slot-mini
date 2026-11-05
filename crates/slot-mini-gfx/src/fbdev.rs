//! The Miyoo Mini Plus framebuffer: `/dev/fb0`, 32 bpp ARGB8888, mounted
//! upside down (Onion reads it back rotated 180°), with a virtual height of
//! two screens so frames can be flipped with `FBIOPAN_DISPLAY`. See
//! docs/PLATFORM.md, "Display".

use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::path::Path;

use crate::canvas::Canvas;

// linux/fb.h
const FBIOGET_VSCREENINFO: libc::c_ulong = 0x4600;
const FBIOPUT_VSCREENINFO: libc::c_ulong = 0x4601;
const FBIOGET_FSCREENINFO: libc::c_ulong = 0x4602;
const FBIOPAN_DISPLAY: libc::c_ulong = 0x4606;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
struct Bitfield {
    offset: u32,
    length: u32,
    msb_right: u32,
}

/// `struct fb_var_screeninfo`.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct VarInfo {
    pub xres: u32,
    pub yres: u32,
    pub xres_virtual: u32,
    pub yres_virtual: u32,
    pub xoffset: u32,
    pub yoffset: u32,
    pub bits_per_pixel: u32,
    grayscale: u32,
    red: Bitfield,
    green: Bitfield,
    blue: Bitfield,
    transp: Bitfield,
    nonstd: u32,
    activate: u32,
    height: u32,
    width: u32,
    accel_flags: u32,
    pixclock: u32,
    left_margin: u32,
    right_margin: u32,
    upper_margin: u32,
    lower_margin: u32,
    hsync_len: u32,
    vsync_len: u32,
    sync: u32,
    vmode: u32,
    rotate: u32,
    colorspace: u32,
    reserved: [u32; 4],
}

/// `struct fb_fix_screeninfo` (only `smem_len` and `line_length` are used).
#[repr(C)]
#[derive(Clone, Copy)]
struct FixInfo {
    id: [u8; 16],
    smem_start: libc::c_ulong,
    smem_len: u32,
    type_: u32,
    type_aux: u32,
    visual: u32,
    xpanstep: u16,
    ypanstep: u16,
    ywrapstep: u16,
    line_length: u32,
    mmio_start: libc::c_ulong,
    mmio_len: u32,
    accel: u32,
    capabilities: u16,
    reserved: [u16; 2],
}

/// Where frames go and how they are laid out in memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub width: u32,
    pub height: u32,
    /// Bytes per row.
    pub stride: u32,
    /// How many screens fit in the mapping (2 for double buffering).
    pub pages: u32,
    /// The panel is mounted upside down.
    pub rotate180: bool,
}

impl Geometry {
    /// The Mini Plus: 640x480, two pages, upside down.
    pub const MINI: Geometry = Geometry {
        width: 640,
        height: 480,
        stride: 640 * 4,
        pages: 2,
        rotate180: true,
    };

    pub fn bytes(&self) -> usize {
        (self.stride * self.height * self.pages) as usize
    }
}

/// Copy a frame into one page of framebuffer memory: rotated 180° if the
/// panel is upside down, centred if the screen is larger than the frame
/// (752x560 panels), black around it.
pub fn write_page(frame: &Canvas, mem: &mut [u32], g: &Geometry, page: u32) {
    let row_px = (g.stride / 4) as usize;
    let page_px = row_px * g.height as usize;
    let base = page as usize * page_px;
    let mem = &mut mem[base..base + page_px];
    let (fw, fh) = (frame.w.min(g.width), frame.h.min(g.height));
    let (ox, oy) = ((g.width - fw) / 2, (g.height - fh) / 2);
    if (fw, fh) != (g.width, g.height) {
        mem.fill(0xFF00_0000);
    }
    for y in 0..fh {
        let src = &frame.px[(y * frame.w) as usize..][..fw as usize];
        let (sy, sx) = (oy + y, ox);
        match g.rotate180 {
            false => {
                let dst = &mut mem[sy as usize * row_px + sx as usize..][..fw as usize];
                dst.copy_from_slice(src);
            }
            true => {
                // Row y lands on row H-1-y, reversed.
                let dy = g.height - 1 - sy;
                let dx_end = g.width - sx; // exclusive end of the mirrored span
                let dst = &mut mem[dy as usize * row_px + (dx_end - fw) as usize..][..fw as usize];
                for (d, s) in dst.iter_mut().rev().zip(src) {
                    *d = *s;
                }
            }
        }
    }
}

pub struct Fbdev {
    file: File,
    map: *mut u32,
    len: usize,
    pub geometry: Geometry,
    var: VarInfo,
    /// The page shown now; the next frame goes to the other one.
    shown: u32,
    /// Whether panning works (a regular file in tests, or a driver without
    /// it, gets single-buffered drawing).
    can_pan: bool,
}

// The mapping is owned by this value alone.
unsafe impl Send for Fbdev {}

impl Fbdev {
    /// Open a framebuffer device and map it.
    pub fn open(path: &Path) -> std::io::Result<Fbdev> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let fd = file.as_raw_fd();
        let mut var = VarInfo::default();
        // SAFETY: the ioctls fill structs of the kernel's layout.
        let mut fix: FixInfo = unsafe { std::mem::zeroed() };
        if unsafe { libc::ioctl(fd, FBIOGET_VSCREENINFO as _, &mut var) } != 0
            || unsafe { libc::ioctl(fd, FBIOGET_FSCREENINFO as _, &mut fix) } != 0
        {
            return Err(std::io::Error::last_os_error());
        }
        if var.bits_per_pixel != 32 {
            return Err(std::io::Error::other(format!(
                "{} bpp, need 32",
                var.bits_per_pixel
            )));
        }
        let stride = if fix.line_length > 0 {
            fix.line_length
        } else {
            var.xres_virtual * 4
        };
        let pages = (var.yres_virtual / var.yres.max(1)).clamp(1, 2);
        let geometry = Geometry {
            width: var.xres,
            height: var.yres,
            stride,
            pages,
            rotate180: true,
        };
        let len = (fix.smem_len as usize).max(geometry.bytes());
        let mut fb = Fbdev::map(file, geometry, len)?;
        fb.var = var;
        fb.shown = var.yoffset / var.yres.max(1);
        fb.can_pan = pages > 1;
        Ok(fb)
    }

    /// Map a plain file laid out like a framebuffer, for tests: no ioctls,
    /// and the "pan" only records which page would be shown.
    pub fn from_file(path: &Path, geometry: Geometry) -> std::io::Result<Fbdev> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        file.set_len(geometry.bytes() as u64)?;
        let mut fb = Fbdev::map(file, geometry, geometry.bytes())?;
        fb.can_pan = false;
        Ok(fb)
    }

    fn map(file: File, geometry: Geometry, len: usize) -> std::io::Result<Fbdev> {
        // SAFETY: a shared mapping of the whole file, unmapped in Drop.
        let map = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if map == libc::MAP_FAILED {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Fbdev {
            file,
            map: map as *mut u32,
            len,
            geometry,
            var: VarInfo::default(),
            shown: 0,
            can_pan: false,
        })
    }

    fn mem(&mut self) -> &mut [u32] {
        // SAFETY: the mapping is `len` bytes, page aligned, and ours.
        unsafe { std::slice::from_raw_parts_mut(self.map, self.len / 4) }
    }

    /// The page the last frame went to.
    pub fn shown_page(&self) -> u32 {
        self.shown
    }

    /// Show `page`. Returns false if the driver takes neither pan ioctl.
    fn pan(&mut self, page: u32) -> bool {
        if !self.can_pan {
            return true;
        }
        let mut var = self.var;
        var.yoffset = page * self.geometry.height;
        var.xoffset = 0;
        let fd = self.file.as_raw_fd();
        // SAFETY: both ioctls read a fb_var_screeninfo.
        if unsafe { libc::ioctl(fd, FBIOPAN_DISPLAY as _, &var) } != 0
            // Some drivers only take the offset through PUT_VSCREENINFO
            // (Onion's display.h does it that way).
            && unsafe { libc::ioctl(fd, FBIOPUT_VSCREENINFO as _, &var) } != 0
        {
            return false;
        }
        self.var = var;
        true
    }

    /// Draw a frame into the hidden page and flip to it.
    pub fn present(&mut self, frame: &Canvas) {
        let g = self.geometry;
        let page = match g.pages > 1 {
            true => 1 - self.shown.min(1),
            false => 0,
        };
        write_page(frame, self.mem(), &g, page);
        if !self.pan(page) {
            // Neither ioctl: draw into the visible page from now on.
            self.can_pan = false;
            let shown = self.shown;
            write_page(frame, self.mem(), &g, shown);
            return;
        }
        self.shown = page;
    }

    /// Before exiting: leave the last frame on page 0 and show page 0.
    ///
    /// Onion's own programs assume the first page. `display_reset()` in
    /// Onion's display.h sets `yoffset = 0` after MainUI, and the in-game
    /// Game Switcher overlay draws through Onion's SDL into that page.
    /// Leaving the panel on page 1 makes the overlay draw where nobody
    /// can see it: a black screen that still takes input.
    pub fn hand_back(&mut self, last: &Canvas) {
        let g = self.geometry;
        write_page(last, self.mem(), &g, 0);
        if self.shown != 0 && self.pan(0) {
            self.shown = 0;
        }
    }
}

impl Drop for Fbdev {
    fn drop(&mut self) {
        // SAFETY: unmapping what `map` mapped.
        unsafe {
            libc::munmap(self.map as *mut libc::c_void, self.len);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Canvas {
        let mut c = Canvas::new(640, 480);
        c.clear(0x000000);
        c.px[0] = 0xFFFF_0000; // top-left red
        c.px[639] = 0xFF00_FF00; // top-right green
        c.px[479 * 640] = 0xFF00_00FF; // bottom-left blue
        c
    }

    #[test]
    fn upside_down_panel_gets_the_frame_rotated() {
        let g = Geometry::MINI;
        let mut mem = vec![0u32; g.bytes() / 4];
        write_page(&frame(), &mut mem, &g, 0);
        assert_eq!(
            mem[640 * 480 - 1],
            0xFFFF_0000,
            "top-left ends bottom-right"
        );
        assert_eq!(mem[640 * 479], 0xFF00_FF00, "top-right ends bottom-left");
        assert_eq!(mem[639], 0xFF00_00FF, "bottom-left ends top-right");
        assert!(mem[640 * 480..].iter().all(|&p| p == 0), "page 1 untouched");
    }

    #[test]
    fn a_larger_panel_centres_the_frame() {
        let g = Geometry {
            width: 752,
            height: 560,
            stride: 752 * 4,
            pages: 1,
            rotate180: false,
        };
        let mut mem = vec![0u32; g.bytes() / 4];
        write_page(&frame(), &mut mem, &g, 0);
        let (ox, oy) = (56, 40);
        assert_eq!(mem[oy * 752 + ox], 0xFFFF_0000);
        assert_eq!(mem[0], 0xFF00_0000, "border is black");
    }

    #[test]
    fn frames_alternate_pages_in_a_file_sized_like_the_framebuffer() {
        let d = tempfile_dir();
        let path = d.join("fb0");
        let mut fb = Fbdev::from_file(&path, Geometry::MINI).unwrap();
        fb.present(&frame());
        assert_eq!(fb.shown_page(), 1);
        drop(fb);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), Geometry::MINI.bytes());
        let px = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
        // Page 1 holds the frame, rotated; little-endian ARGB is B, G, R, A.
        let page1 = 640 * 480;
        assert_eq!(px(page1 + 640 * 480 - 1), 0xFFFF_0000);
        assert_eq!(
            &bytes[(page1 + 640 * 480 - 1) * 4..][..4],
            &[0, 0, 255, 255]
        );
        let mut fb = Fbdev::from_file(&path, Geometry::MINI).unwrap();
        fb.shown = 1;
        fb.present(&frame());
        assert_eq!(fb.shown_page(), 0);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn hand_back_leaves_the_last_frame_on_page_zero() {
        let d = tempfile_dir();
        let path = d.join("fb0");
        let mut fb = Fbdev::from_file(&path, Geometry::MINI).unwrap();
        fb.present(&frame());
        assert_eq!(fb.shown_page(), 1, "an odd number of frames ends on page 1");
        let mut last = frame();
        last.px[1] = 0xFF12_3456;
        fb.hand_back(&last);
        assert_eq!(fb.shown_page(), 0);
        drop(fb);
        let bytes = std::fs::read(&path).unwrap();
        let px = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
        assert_eq!(
            px(640 * 480 - 2),
            0xFF12_3456,
            "page 0 holds the last frame, rotated"
        );
        let _ = std::fs::remove_dir_all(d);
    }

    fn tempfile_dir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "slot-mini-fb-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
}
