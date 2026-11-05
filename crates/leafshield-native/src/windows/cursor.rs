//! Tint copies of the current system cursors; never persist a pointer scheme.
use leafshield_core::colour::{self, Gains, NEUTRAL};
use std::{mem::size_of, ptr};
use windows_sys::Win32::{Graphics::Gdi::*, System::Registry::*, UI::WindowsAndMessaging::*};

const ERROR: &str = "鼠标指针同步失败，屏幕调节仍可使用";
const CONFLICT: &str = "鼠标指针已由系统或其他程序更改，部分指针暂停同步";
// Busy/background-working pointers are animated; leave them untouched.
const CURSORS: [(u32, &str); 12] = [
    (OCR_NORMAL, "Arrow"),
    (OCR_IBEAM, "IBeam"),
    (OCR_CROSS, "Crosshair"),
    (OCR_UP, "UpArrow"),
    (OCR_SIZENWSE, "SizeNWSE"),
    (OCR_SIZENESW, "SizeNESW"),
    (OCR_SIZEWE, "SizeWE"),
    (OCR_SIZENS, "SizeNS"),
    (OCR_SIZEALL, "SizeAll"),
    (OCR_NO, "No"),
    (OCR_HAND, "Hand"),
    (OCR_HELP, "Help"),
];

struct Cursor(HCURSOR);
impl Drop for Cursor {
    fn drop(&mut self) {
        unsafe { DestroyCursor(self.0) };
    }
}
impl Cursor {
    fn install(self, id: u32) -> Result<(), &'static str> {
        // SetSystemCursor consumes and destroys the supplied private cursor.
        let handle = self.0;
        std::mem::forget(self);
        if unsafe { SetSystemCursor(handle, id) } == 0 {
            Err(ERROR)
        } else {
            Ok(())
        }
    }
}

struct Bitmap(HBITMAP);
impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe { DeleteObject(self.0) };
    }
}

fn bitmap_info(width: i32, height: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn read_bitmap(bitmap: HBITMAP) -> Result<(i32, i32, Vec<u8>), &'static str> {
    let mut object = BITMAP::default();
    if unsafe {
        GetObjectW(
            bitmap,
            size_of::<BITMAP>() as i32,
            ptr::from_mut(&mut object).cast(),
        )
    } == 0
        || !(1..=1024).contains(&object.bmWidth)
        || !(1..=2048).contains(&object.bmHeight)
    {
        return Err(ERROR);
    }
    let (width, height) = (object.bmWidth, object.bmHeight);
    let mut pixels = vec![0; (width * height * 4) as usize];
    let dc = unsafe { CreateCompatibleDC(ptr::null_mut()) };
    if dc.is_null() {
        return Err(ERROR);
    }
    let lines = unsafe {
        GetDIBits(
            dc,
            bitmap,
            0,
            height as u32,
            pixels.as_mut_ptr().cast(),
            &mut bitmap_info(width, height),
            DIB_RGB_COLORS,
        )
    };
    unsafe { DeleteDC(dc) };
    if lines != height {
        return Err(ERROR);
    }
    Ok((width, height, pixels))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Image {
    width: i32,
    height: i32,
    hotspot: (u32, u32),
    mask: Vec<u8>,
    pixels: Vec<u8>,
}
impl Image {
    fn read(cursor: HCURSOR) -> Result<Self, &'static str> {
        let mut info = ICONINFO::default();
        if unsafe { GetIconInfo(cursor, &mut info) } == 0 {
            return Err(ERROR);
        }
        let mask = Bitmap(info.hbmMask);
        let color = Bitmap(info.hbmColor);
        let (width, mask_height, mut mask) = read_bitmap(mask.0)?;
        let (height, pixels) = if color.0.is_null() {
            if mask_height % 2 != 0 {
                return Err(ERROR);
            }
            let height = mask_height / 2;
            let pixels = mask.split_off((width * height * 4) as usize);
            (height, pixels)
        } else {
            let (color_width, height, pixels) = read_bitmap(color.0)?;
            if color_width != width || height != mask_height {
                return Err(ERROR);
            }
            (height, pixels)
        };
        Ok(Self {
            width,
            height,
            hotspot: (info.xHotspot, info.yHotspot),
            mask,
            pixels,
        })
    }

    fn system(id: u32) -> Result<Self, &'static str> {
        Self::read(unsafe { LoadCursorW(ptr::null_mut(), id as usize as *const u16) })
    }

    fn tinted(&self, rgb: Gains) -> Self {
        let mut next = self.clone();
        let alpha = self.pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 0);
        for (pixel, mask) in next
            .pixels
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(self.mask.as_chunks::<4>().0)
        {
            // Keep transparent and XOR/inverted pixels intact in legacy cursors.
            if alpha || mask[0] == 0 {
                for channel in 0..3 {
                    pixel[channel] = (pixel[channel] as f32 * rgb[2 - channel]).round() as u8;
                }
            }
        }
        next
    }

    fn create(&self) -> Result<Cursor, &'static str> {
        let mut bits = ptr::null_mut();
        let color = Bitmap(unsafe {
            CreateDIBSection(
                ptr::null_mut(),
                &bitmap_info(self.width, self.height),
                DIB_RGB_COLORS,
                &mut bits,
                ptr::null_mut(),
                0,
            )
        });
        if color.0.is_null() {
            return Err(ERROR);
        }
        unsafe { ptr::copy_nonoverlapping(self.pixels.as_ptr(), bits.cast(), self.pixels.len()) };
        // CreateBitmap monochrome rows are WORD-aligned (DIB rows are DWORD-aligned).
        let stride = (self.width as usize).div_ceil(16) * 2;
        let mut mask_bits = vec![0u8; stride * self.height as usize];
        for y in 0..self.height as usize {
            for x in 0..self.width as usize {
                if self.mask[(y * self.width as usize + x) * 4] != 0 {
                    mask_bits[y * stride + x / 8] |= 0x80 >> (x % 8);
                }
            }
        }
        let mask = Bitmap(unsafe {
            CreateBitmap(self.width, self.height, 1, 1, mask_bits.as_ptr().cast())
        });
        if mask.0.is_null() {
            return Err(ERROR);
        }
        let cursor = unsafe {
            CreateIconIndirect(&ICONINFO {
                fIcon: 0,
                xHotspot: self.hotspot.0,
                yHotspot: self.hotspot.1,
                hbmMask: mask.0,
                hbmColor: color.0,
            })
        };
        if cursor.is_null() {
            Err(ERROR)
        } else {
            Ok(Cursor(cursor))
        }
    }
}

fn animated(name: &str) -> Result<bool, &'static str> {
    let key: Vec<u16> = "Control Panel\\Cursors\0".encode_utf16().collect();
    let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let mut path = [0u16; 2048];
    let mut bytes = size_of_val(&path) as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND,
            ptr::null_mut(),
            path.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    match result {
        0 => Ok(String::from_utf16_lossy(
            &path[..path.iter().position(|c| *c == 0).unwrap_or(path.len())],
        )
        .trim()
        .to_ascii_lowercase()
        .ends_with(".ani")),
        2 | 3 => Ok(false), // No custom file or pointer-scheme key for this user.
        _ => Err(ERROR),
    }
}

struct Slot {
    id: u32,
    original: Cursor,
    image: Image,
    last: Option<Image>,
    available: bool,
}
impl Slot {
    fn restore(&mut self) -> Result<(), &'static str> {
        self.restore_with(Image::system, Cursor::install)
    }

    fn restore_with(
        &mut self,
        current: impl FnOnce(u32) -> Result<Image, &'static str>,
        install: impl FnOnce(Cursor, u32) -> Result<(), &'static str>,
    ) -> Result<(), &'static str> {
        if let Some(last) = &self.last {
            if current(self.id)? == *last {
                let copy = unsafe { CopyIcon(self.original.0) };
                if copy.is_null() {
                    return Err(ERROR);
                }
                install(Cursor(copy), self.id)?;
            }
            // A newer system/user pointer takes precedence over our snapshot.
            self.last = None;
        }
        Ok(())
    }

    fn update(
        &mut self,
        rgb: Gains,
        current: Image,
        install: impl FnOnce(Cursor, u32) -> Result<(), &'static str>,
    ) -> Result<(), &'static str> {
        if !self.available {
            return Err(CONFLICT);
        }
        if current != *self.last.as_ref().unwrap_or(&self.image) {
            self.last = None;
            self.available = false;
            return Err(CONFLICT);
        }
        // Always start with the snapshot, so repeated slider updates cannot compound tint.
        let next = self.image.tinted(rgb);
        if self.last.as_ref() == Some(&next) {
            return Ok(());
        }
        let cursor = next.create()?;
        let installed = Image::read(cursor.0)?;
        install(cursor, self.id)?;
        self.last = Some(installed);
        Ok(())
    }
}

#[derive(Default)]
pub struct Session {
    slots: Vec<Slot>,
}
impl Session {
    pub fn apply(&mut self, rgb: Gains) -> Result<(), &'static str> {
        if colour::same(rgb, NEUTRAL) {
            let mut result = Ok(());
            for slot in &mut self.slots {
                if let Err(error) = slot.restore() {
                    result = Err(error);
                }
            }
            if result.is_ok() {
                self.slots.clear();
            }
            return result;
        }
        if self.slots.is_empty() {
            let mut slots = Vec::new();
            for (id, name) in CURSORS {
                if animated(name)? {
                    continue;
                }
                let shared = unsafe { LoadCursorW(ptr::null_mut(), id as usize as *const u16) };
                let copy = unsafe { CopyIcon(shared) };
                if copy.is_null() {
                    return Err(ERROR);
                }
                let original = Cursor(copy);
                let image = Image::read(original.0)?;
                slots.push(Slot {
                    id,
                    original,
                    image,
                    last: None,
                    available: true,
                });
            }
            self.slots = slots;
        }
        let mut result = Ok(());
        for slot in &mut self.slots {
            if !slot.available {
                result = Err(CONFLICT);
                continue;
            }
            let update = Image::system(slot.id)
                .and_then(|current| slot.update(rgb, current, Cursor::install));
            if let Err(error) = update {
                result = Err(error);
            }
        }
        result
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        for slot in &mut self.slots {
            let _ = slot.restore();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tint_preserves_alpha_hotspot_shape_and_original_pixels() {
        let image = Image {
            width: 2,
            height: 2,
            hotspot: (1, 0),
            mask: vec![0; 16],
            pixels: vec![
                255, 255, 255, 255, 40, 80, 120, 128, 0, 0, 0, 255, 0, 0, 0, 0,
            ],
        };
        let next = image.tinted([0.8, 0.6, 0.4]);
        assert_eq!(
            next.pixels,
            [
                102, 153, 204, 255, 16, 48, 96, 128, 0, 0, 0, 255, 0, 0, 0, 0
            ]
        );
        let cursor = next.create().unwrap();
        assert_eq!(Image::read(cursor.0).unwrap(), next);
        assert_eq!(image.pixels[0], 255);
        assert_eq!(image.tinted(NEUTRAL), image);
    }

    #[test]
    fn legacy_transparency_and_inversion_remain_intact() {
        let image = Image {
            width: 3,
            height: 1,
            hotspot: (0, 0),
            mask: vec![0, 0, 0, 0, 255, 255, 255, 0, 255, 255, 255, 0],
            pixels: vec![255, 255, 255, 0, 0, 0, 0, 0, 255, 255, 255, 0],
        };
        let next = image.tinted([1.0, 0.5, 0.2]);
        assert_eq!(next.pixels, [51, 128, 255, 0, 0, 0, 0, 0, 255, 255, 255, 0]);
        assert_eq!(Image::read(next.create().unwrap().0).unwrap(), next);
    }

    #[test]
    fn current_system_cursors_can_be_tinted_without_installing_them() {
        for (id, name) in CURSORS {
            if animated(name).unwrap() {
                continue;
            }
            let original = Image::system(id).unwrap();
            let next = original.tinted(colour::gains(3500, 65, true));
            let actual = Image::read(next.create().unwrap().0).unwrap();
            assert_eq!(actual, next, "{name} must preserve pixels and geometry");
            assert_eq!(
                Image::system(id).unwrap(),
                original,
                "{name} must remain unchanged"
            );
        }
    }

    fn test_slot() -> Slot {
        let image = Image {
            width: 3,
            height: 2,
            hotspot: (1, 1),
            // Asymmetric rows catch mask stride and orientation errors.
            mask: vec![
                0, 0, 0, 0, 255, 255, 255, 0, 0, 0, 0, 0, 255, 255, 255, 0, 0, 0, 0, 0, 255, 255,
                255, 0,
            ],
            pixels: [255, 255, 255, 255].repeat(6),
        };
        let original = image.create().unwrap();
        assert_eq!(Image::read(original.0).unwrap(), image);
        Slot {
            id: OCR_NORMAL,
            original,
            image,
            last: None,
            available: true,
        }
    }

    #[test]
    fn repeated_updates_do_not_compound_and_restore_copies_original() {
        let mut slot = test_slot();
        let original = slot.image.clone();
        let rgb = colour::gains(3500, 65, true);
        let mut current = slot.image.clone();
        for index in 0..3 {
            slot.update(rgb, current.clone(), |cursor, id| {
                assert_eq!(index, 0, "unchanged frames must not replace system cursors");
                assert_eq!(id, OCR_NORMAL);
                current = Image::read(cursor.0).unwrap();
                Ok(())
            })
            .unwrap();
        }
        assert_eq!(current, slot.image.tinted(rgb));
        let restored = std::cell::Cell::new(false);
        slot.restore_with(
            |_| Ok(current),
            |cursor, id| {
                assert_eq!(id, OCR_NORMAL);
                assert_eq!(Image::read(cursor.0).unwrap(), original);
                restored.set(true);
                Ok(())
            },
        )
        .unwrap();
        assert!(restored.get());
        assert!(slot.last.is_none());
    }

    #[test]
    fn external_theme_change_is_neither_overwritten_nor_restored() {
        let mut slot = test_slot();
        let rgb = colour::gains(3500, 65, true);
        slot.update(rgb, slot.image.clone(), |_, _| Ok(())).unwrap();
        let mut external = slot.image.clone();
        external.hotspot = (0, 0);
        // Even when we have not polled the new theme yet, exit must preserve it.
        slot.restore_with(
            |_| Ok(external.clone()),
            |_, _| panic!("new theme must survive exit"),
        )
        .unwrap();
        assert!(slot.last.is_none());
        assert_eq!(
            slot.update(rgb, external, |_, _| panic!(
                "new theme must survive adjustment"
            )),
            Err(CONFLICT)
        );
        assert!(!slot.available);
        assert_eq!(
            slot.update(rgb, slot.image.clone(), |_, _| panic!(
                "conflicted slot stays paused"
            )),
            Err(CONFLICT)
        );
    }
}
