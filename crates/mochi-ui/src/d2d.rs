use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::path::Path;

use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_BEZIER_SEGMENT,
    D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_CLOSED,
    D2D1_FIGURE_END_OPEN, D2D1_GRADIENT_STOP, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE, D2D1_ARC_SIZE_SMALL,
    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_BITMAP_PROPERTIES, D2D1_CAP_STYLE_ROUND,
    D2D1_DASH_STYLE_CUSTOM, D2D1_ELLIPSE, D2D1_EXTEND_MODE_CLAMP,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_LAYER_OPTIONS_NONE,
    D2D1_LAYER_PARAMETERS, D2D1_LINE_JOIN_ROUND, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
    D2D1_QUADRATIC_BEZIER_SEGMENT, D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_RENDER_TARGET_USAGE_NONE,
    D2D1_ROUNDED_RECT, D2D1_STROKE_STYLE_PROPERTIES, D2D1_SWEEP_DIRECTION_CLOCKWISE,
    D2D1CreateFactory, ID2D1Bitmap, ID2D1DCRenderTarget, ID2D1Factory, ID2D1Geometry, ID2D1Layer,
    ID2D1LinearGradientBrush, ID2D1PathGeometry, ID2D1RadialGradientBrush, ID2D1SolidColorBrush,
    ID2D1StrokeStyle,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_FEATURE, DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE, DWRITE_TRIMMING,
    DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP,
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, IDWriteTextLayout, IDWriteTypography,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, HBITMAP, HDC,
    HGDIOBJ, SelectObject,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory,
    WICBitmapDitherTypeNone, WICBitmapInterpolationModeFant, WICBitmapPaletteTypeMedianCut,
    WICDecodeMetadataCacheOnLoad,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow};
use windows_core::{Interface, PCWSTR};
use windows_numerics::{Matrix3x2, Vector2};

use mochi_winrt::mochi_media;
use mochi_winrt::native::to_wide;

use crate::theme::{Color, DecodedFrame};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontKind {
    Ui,
    Display,
    Icon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VAlign {
    Top,
    Center,
}

type FormatKey = (u8, u32, bool, u8, u8, bool, bool);
type NotchCacheEntry = ((u32, u32, u32), ID2D1PathGeometry);

pub struct D2dFactories {
    pub d2d: ID2D1Factory,
    pub dwrite: IDWriteFactory,
    pub wic: IWICImagingFactory,
    pub round_stroke: ID2D1StrokeStyle,
    pub dashed_stroke: ID2D1StrokeStyle,
    pub pointer_geom: ID2D1PathGeometry,
    tabular_typo: Option<IDWriteTypography>,
    format_cache: RefCell<HashMap<FormatKey, IDWriteTextFormat>>,
    notch_cache: RefCell<Option<NotchCacheEntry>>,
}

impl D2dFactories {
    pub fn new() -> windows_core::Result<Self> {
        // SAFETY: D2D1CreateFactory creates a single-threaded Direct2D factory.
        let d2d: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)? };
        // SAFETY: DWriteCreateFactory creates a shared DirectWrite factory.
        let dwrite: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)? };
        // SAFETY: CoCreateInstance creates the standard WIC imaging factory.
        let wic: IWICImagingFactory =
            unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)? };

        let round_props = D2D1_STROKE_STYLE_PROPERTIES {
            startCap: D2D1_CAP_STYLE_ROUND,
            endCap: D2D1_CAP_STYLE_ROUND,
            lineJoin: D2D1_LINE_JOIN_ROUND,
            ..Default::default()
        };
        // SAFETY: `d2d` is valid.
        let round_stroke = unsafe { d2d.CreateStrokeStyle(&round_props, None)? };

        let dash_props = D2D1_STROKE_STYLE_PROPERTIES {
            dashStyle: D2D1_DASH_STYLE_CUSTOM,
            ..Default::default()
        };
        let dashes = [4.0_f32, 3.0_f32];
        // SAFETY: `d2d` is valid and `dashes` is a valid slice.
        let dashed_stroke = unsafe { d2d.CreateStrokeStyle(&dash_props, Some(&dashes))? };

        let pointer_geom = build_pointer_geometry(&d2d)?;

        // SAFETY: Creating shared tabular-figures typography object.
        let tabular_typo = unsafe { dwrite.CreateTypography() }.ok().inspect(|typo| {
            let feat = DWRITE_FONT_FEATURE {
                nameTag: DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES,
                parameter: 1,
            };
            // SAFETY: `typo` is valid.
            unsafe {
                let _ = typo.AddFontFeature(feat);
            }
        });

        Ok(Self {
            d2d,
            dwrite,
            wic,
            round_stroke,
            dashed_stroke,
            pointer_geom,
            tabular_typo,
            format_cache: RefCell::new(HashMap::new()),
            notch_cache: RefCell::new(None),
        })
    }

    pub fn create_dashed_stroke(&self, dash_offset: f32) -> Option<ID2D1StrokeStyle> {
        let props = D2D1_STROKE_STYLE_PROPERTIES {
            startCap: D2D1_CAP_STYLE_ROUND,
            endCap: D2D1_CAP_STYLE_ROUND,
            dashCap: D2D1_CAP_STYLE_ROUND,
            lineJoin: D2D1_LINE_JOIN_ROUND,
            dashStyle: D2D1_DASH_STYLE_CUSTOM,
            dashOffset: dash_offset,
            ..Default::default()
        };
        let dashes = [5.0_f32, 4.0_f32];
        // SAFETY: `self.d2d` is valid and `dashes` is a valid slice.
        unsafe { self.d2d.CreateStrokeStyle(&props, Some(&dashes)) }.ok()
    }

    pub fn create_text_format(
        &self,
        kind: FontKind,
        size: f32,
        semibold: bool,
    ) -> windows_core::Result<IDWriteTextFormat> {
        let family = match kind {
            FontKind::Ui => "Segoe UI Variable Text",
            FontKind::Display => "Segoe UI Variable Display",
            FontKind::Icon => "Segoe Fluent Icons",
        };
        let wfamily = to_wide(family);
        let wlocale = to_wide("en-US");
        let weight = if semibold {
            DWRITE_FONT_WEIGHT_SEMI_BOLD
        } else {
            DWRITE_FONT_WEIGHT_NORMAL
        };
        // SAFETY: `dwrite` is valid and strings are NUL-terminated UTF-16.
        unsafe {
            self.dwrite.CreateTextFormat(
                PCWSTR(wfamily.as_ptr()),
                None,
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                PCWSTR(wlocale.as_ptr()),
            )
        }
    }

    #[expect(clippy::too_many_arguments, reason = "DirectWrite layout helper")]
    pub fn create_text_layout(
        &self,
        text: &str,
        kind: FontKind,
        size: f32,
        semibold: bool,
        max_w: f32,
        max_h: f32,
        halign: HAlign,
        valign: VAlign,
        tabular: bool,
        wrap: bool,
        ellipsis: bool,
        strikethrough: bool,
    ) -> windows_core::Result<IDWriteTextLayout> {
        let key: FormatKey = (
            kind as u8,
            size.to_bits(),
            semibold,
            halign as u8,
            valign as u8,
            wrap,
            ellipsis,
        );
        let fmt = if let Some(cached) = self.format_cache.borrow().get(&key).cloned() {
            cached
        } else {
            let fmt = self.create_text_format(kind, size, semibold)?;
            // SAFETY: `fmt` is valid.
            unsafe {
                let _ = fmt.SetTextAlignment(match halign {
                    HAlign::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                    HAlign::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
                    HAlign::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
                });
                let _ = fmt.SetParagraphAlignment(match valign {
                    VAlign::Top => DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
                    VAlign::Center => DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
                });
                let _ = fmt.SetWordWrapping(if wrap {
                    DWRITE_WORD_WRAPPING_WRAP
                } else {
                    DWRITE_WORD_WRAPPING_NO_WRAP
                });
                if ellipsis {
                    let trim = DWRITE_TRIMMING {
                        granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                        delimiter: 0,
                        delimiterCount: 0,
                    };
                    if let Ok(sign) = self.dwrite.CreateEllipsisTrimmingSign(&fmt) {
                        let _ = fmt.SetTrimming(&trim, &sign);
                    }
                }
            }
            let mut cache = self.format_cache.borrow_mut();
            if cache.len() >= 64 {
                cache.clear();
            }
            cache.insert(key, fmt.clone());
            fmt
        };
        let utf16: Vec<u16> = text.encode_utf16().collect();
        // SAFETY: `dwrite` and `fmt` are valid.
        let layout = unsafe {
            self.dwrite
                .CreateTextLayout(&utf16, &fmt, max_w.max(1.0), max_h.max(1.0))?
        };
        let range = DWRITE_TEXT_RANGE {
            startPosition: 0,
            length: utf16.len() as u32,
        };
        if tabular
            && !utf16.is_empty()
            && let Some(ref typo) = self.tabular_typo
        {
            // SAFETY: `typo` and `layout` are valid.
            unsafe {
                let _ = layout.SetTypography(typo, range);
            }
        }
        if strikethrough && !utf16.is_empty() {
            // SAFETY: `layout` is valid.
            unsafe {
                let _ = layout.SetStrikethrough(true, range);
            }
        }
        Ok(layout)
    }

    #[expect(clippy::too_many_arguments, reason = "DirectWrite measure helper")]
    pub fn measure_text(
        &self,
        text: &str,
        kind: FontKind,
        size: f32,
        semibold: bool,
        tabular: bool,
        max_w: f32,
        wrap: bool,
    ) -> (f32, f32) {
        let Ok(layout) = self.create_text_layout(
            text,
            kind,
            size,
            semibold,
            max_w,
            10_000.0,
            HAlign::Left,
            VAlign::Top,
            tabular,
            wrap,
            false,
            false,
        ) else {
            return (0.0, 0.0);
        };
        let mut metrics = DWRITE_TEXT_METRICS::default();
        // SAFETY: `layout` is valid.
        if unsafe { layout.GetMetrics(&raw mut metrics) }.is_ok() {
            (metrics.widthIncludingTrailingWhitespace, metrics.height)
        } else {
            (0.0, 0.0)
        }
    }

    pub fn build_notch_geometry(
        &self,
        width: f64,
        height: f64,
        radius: f64,
    ) -> Option<ID2D1PathGeometry> {
        let w = (if width.is_finite() { width } else { 120.0 }).max(24.0) as f32;
        let h = (if height.is_finite() { height } else { 30.0 }).max(14.0) as f32;
        let r = (if radius.is_finite() { radius } else { 15.0 })
            .clamp(0.0, f64::from(w.min(h)) / 2.0) as f32;
        let key = (w.to_bits(), h.to_bits(), r.to_bits());
        if let Some((k, ref geom)) = *self.notch_cache.borrow()
            && k == key
        {
            return Some(geom.clone());
        }
        let e = 8.0_f32;
        let ear_h = 6.0_f32;

        // SAFETY: `d2d` is valid.
        let path = unsafe { self.d2d.CreatePathGeometry() }.ok()?;
        // SAFETY: `path` is valid and sink is closed before returning.
        let sink = unsafe { path.Open() }.ok()?;
        // SAFETY: Drawing closed notch figure matching MainWindow.BuildNotchGeometry.
        unsafe {
            sink.BeginFigure(Vector2::new(0.0, 0.0), D2D1_FIGURE_BEGIN_FILLED);
            sink.AddLine(Vector2::new(w + 2.0 * e, 0.0));
            sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                point1: Vector2::new(w + e + 0.55 * e, 0.0),
                point2: Vector2::new(w + e, 0.45 * ear_h),
                point3: Vector2::new(w + e, ear_h),
            });
            sink.AddLine(Vector2::new(w + e, h - r));
            if r > 0.01 {
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: Vector2::new(w + e - r, h),
                    size: D2D_SIZE_F {
                        width: r,
                        height: r,
                    },
                    rotationAngle: 0.0,
                    sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                    arcSize: D2D1_ARC_SIZE_SMALL,
                });
            }
            sink.AddLine(Vector2::new(e + r, h));
            if r > 0.01 {
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: Vector2::new(e, h - r),
                    size: D2D_SIZE_F {
                        width: r,
                        height: r,
                    },
                    rotationAngle: 0.0,
                    sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                    arcSize: D2D1_ARC_SIZE_SMALL,
                });
            }
            sink.AddLine(Vector2::new(e, ear_h));
            sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                point1: Vector2::new(e, 0.45 * ear_h),
                point2: Vector2::new(0.45 * e, 0.0),
                point3: Vector2::new(0.0, 0.0),
            });
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            let _ = sink.Close();
        }
        *self.notch_cache.borrow_mut() = Some((key, path.clone()));
        Some(path)
    }

    pub fn build_arc_geometry(
        &self,
        progress: f64,
        size: f32,
        stroke: f32,
    ) -> Option<ID2D1PathGeometry> {
        let p = progress.clamp(0.0, 0.9999);
        if p <= 0.0 {
            return None;
        }
        let radius = (size - stroke) / 2.0;
        let center = size / 2.0;
        let angle = (p * 2.0 * std::f64::consts::PI) as f32;
        let start = Vector2::new(center, center - radius);
        let end = Vector2::new(center + radius * angle.sin(), center - radius * angle.cos());
        // SAFETY: `d2d` is valid.
        let path = unsafe { self.d2d.CreatePathGeometry() }.ok()?;
        // SAFETY: `path` is valid.
        let sink = unsafe { path.Open() }.ok()?;
        // SAFETY: Drawing clockwise arc from 12 o'clock.
        unsafe {
            sink.BeginFigure(start, D2D1_FIGURE_BEGIN_HOLLOW);
            sink.AddArc(&D2D1_ARC_SEGMENT {
                point: end,
                size: D2D_SIZE_F {
                    width: radius,
                    height: radius,
                },
                rotationAngle: 0.0,
                sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                arcSize: if p > 0.5 {
                    D2D1_ARC_SIZE_LARGE
                } else {
                    D2D1_ARC_SIZE_SMALL
                },
            });
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
            let _ = sink.Close();
        }
        Some(path)
    }

    pub fn decode_image_bytes(&self, bytes: &[u8]) -> Option<DecodedFrame> {
        self.decode_thumbnail_bytes(bytes, 0)
    }

    pub fn decode_thumbnail_bytes(&self, bytes: &[u8], max_dim: u32) -> Option<DecodedFrame> {
        // SAFETY: Creating WIC stream and decoder from memory slice.
        unsafe {
            let stream = self.wic.CreateStream().ok()?;
            stream.InitializeFromMemory(bytes).ok()?;
            let decoder = self
                .wic
                .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnLoad)
                .ok()?;
            let frame = decoder.GetFrame(0).ok()?;
            let mut w = 0u32;
            let mut h = 0u32;
            frame.GetSize(&raw mut w, &raw mut h).ok()?;
            if w == 0 || h == 0 || w > 16384 || h > 16384 {
                return None;
            }
            let effective_max = if max_dim > 0 { max_dim.min(4096) } else { 4096 };
            let (tw, th) = if w > effective_max || h > effective_max {
                if w >= h {
                    (
                        effective_max,
                        ((u64::from(h) * u64::from(effective_max)) / u64::from(w)).max(1) as u32,
                    )
                } else {
                    (
                        ((u64::from(w) * u64::from(effective_max)) / u64::from(h)).max(1) as u32,
                        effective_max,
                    )
                }
            } else {
                (w, h)
            };
            let stride = tw.checked_mul(4)?;
            let buf_len = usize::try_from(stride.checked_mul(th)?).ok()?;
            let conv = self.wic.CreateFormatConverter().ok()?;
            if tw != w || th != h {
                let scaler = self.wic.CreateBitmapScaler().ok()?;
                scaler
                    .Initialize(&frame, tw, th, WICBitmapInterpolationModeFant)
                    .ok()?;
                conv.Initialize(
                    &scaler,
                    &GUID_WICPixelFormat32bppPBGRA,
                    WICBitmapDitherTypeNone,
                    None,
                    0.0,
                    WICBitmapPaletteTypeMedianCut,
                )
                .ok()?;
            } else {
                conv.Initialize(
                    &frame,
                    &GUID_WICPixelFormat32bppPBGRA,
                    WICBitmapDitherTypeNone,
                    None,
                    0.0,
                    WICBitmapPaletteTypeMedianCut,
                )
                .ok()?;
            }
            let mut bgra = vec![0u8; buf_len];
            conv.CopyPixels(std::ptr::null(), stride, &mut bgra).ok()?;
            Some(DecodedFrame {
                width: tw,
                height: th,
                bgra_premul: bgra,
            })
        }
    }

    pub fn extract_accent_from_bytes(&self, bytes: &[u8]) -> Color {
        // SAFETY: Scaling image to 12x12 PBGRA via WIC and passing to mochi_media.
        let scaled = unsafe {
            (|| -> Option<Vec<i64>> {
                let stream = self.wic.CreateStream().ok()?;
                stream.InitializeFromMemory(bytes).ok()?;
                let decoder = self
                    .wic
                    .CreateDecoderFromStream(
                        &stream,
                        std::ptr::null(),
                        WICDecodeMetadataCacheOnLoad,
                    )
                    .ok()?;
                let frame = decoder.GetFrame(0).ok()?;
                let scaler = self.wic.CreateBitmapScaler().ok()?;
                scaler
                    .Initialize(&frame, 12, 12, WICBitmapInterpolationModeFant)
                    .ok()?;
                let conv = self.wic.CreateFormatConverter().ok()?;
                conv.Initialize(
                    &scaler,
                    &GUID_WICPixelFormat32bppPBGRA,
                    WICBitmapDitherTypeNone,
                    None,
                    0.0,
                    WICBitmapPaletteTypeMedianCut,
                )
                .ok()?;
                let mut bgra = vec![0u8; 12 * 12 * 4];
                conv.CopyPixels(std::ptr::null(), 12 * 4, &mut bgra).ok()?;
                Some(bgra.into_iter().map(i64::from).collect())
            })()
        };
        let res = mochi_media::port::extract_accent_color(scaled).ok();
        if let Some(v) = res
            && let (Some(r), Some(g), Some(b)) = (
                v.get("R").and_then(|x| x.as_u64()),
                v.get("G").and_then(|x| x.as_u64()),
                v.get("B").and_then(|x| x.as_u64()),
            )
        {
            return Color::rgb(r as u8, g as u8, b as u8);
        }
        crate::theme::DEFAULT_ACCENT
    }
}

pub struct D2dSurface {
    pub rt: ID2D1DCRenderTarget,
    hdc: HDC,
    hbmp: HBITMAP,
    old_obj: HGDIOBJ,
    bits: *mut u8,
    pub pixel_w: i32,
    pub pixel_h: i32,
    cap_w: i32,
    cap_h: i32,
    pub scale: f64,
    solid_brush: Option<ID2D1SolidColorBrush>,
    shadow_cache: HashMap<(u32, u8), [ID2D1Bitmap; 9]>,
    image_bitmaps: HashMap<u64, ID2D1Bitmap>,
    layer_pool: Vec<ID2D1Layer>,
    active_layers: Vec<ID2D1Layer>,
}

impl D2dSurface {
    pub fn new(
        factories: &D2dFactories,
        pixel_w: i32,
        pixel_h: i32,
        scale: f64,
    ) -> windows_core::Result<Self> {
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: (96.0 * scale) as f32,
            dpiY: (96.0 * scale) as f32,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        // SAFETY: `factories.d2d` is valid.
        let rt = unsafe { factories.d2d.CreateDCRenderTarget(&props)? };
        // SAFETY: Creating reusable solid color brush.
        let solid_brush = unsafe {
            rt.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                },
                None,
            )
        }
        .ok();
        // SAFETY: Creating memory DC for DIB section.
        let hdc = unsafe { CreateCompatibleDC(None) };
        let mut surf = Self {
            rt,
            hdc,
            hbmp: HBITMAP::default(),
            old_obj: HGDIOBJ::default(),
            bits: std::ptr::null_mut(),
            pixel_w: 0,
            pixel_h: 0,
            cap_w: 0,
            cap_h: 0,
            scale,
            solid_brush,
            shadow_cache: HashMap::new(),
            image_bitmaps: HashMap::new(),
            layer_pool: Vec::new(),
            active_layers: Vec::new(),
        };
        surf.ensure_size(pixel_w.max(1), pixel_h.max(1), scale);
        Ok(surf)
    }

    pub fn ensure_size(&mut self, pixel_w: i32, pixel_h: i32, scale: f64) {
        let pw = pixel_w.max(1);
        let ph = pixel_h.max(1);
        if (scale - self.scale).abs() > 0.001 {
            self.scale = scale;
            let dpi = (96.0 * scale) as f32;
            // SAFETY: `self.rt` is valid.
            unsafe {
                self.rt.SetDpi(dpi, dpi);
            }
        }
        self.pixel_w = pw;
        self.pixel_h = ph;
        if pw <= self.cap_w && ph <= self.cap_h && !self.hbmp.is_invalid() {
            return;
        }
        let new_cw = pw.max(self.cap_w);
        let new_ch = ph.max(self.cap_h);
        self.alloc_dib(new_cw, new_ch);
    }

    /// Shrinks the backing DIBSection when the settled window is less than half the peak area.
    pub fn shrink_to_fit(&mut self, pixel_w: i32, pixel_h: i32) {
        let pw = pixel_w.max(1);
        let ph = pixel_h.max(1);
        let cur_area = i64::from(self.cap_w) * i64::from(self.cap_h);
        let need_area = i64::from(pw) * i64::from(ph);
        if cur_area > need_area * 2 && !self.hbmp.is_invalid() {
            self.pixel_w = pw;
            self.pixel_h = ph;
            self.alloc_dib(pw, ph);
        }
    }

    fn alloc_dib(&mut self, new_cw: i32, new_ch: i32) {
        self.free_dib();
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: new_cw,
                biHeight: -new_ch,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        // SAFETY: Creating top-down 32-bit BGRA DIB section.
        if let Ok(hbmp) = unsafe {
            CreateDIBSection(Some(self.hdc), &bmi, DIB_RGB_COLORS, &raw mut bits, None, 0)
        } {
            // SAFETY: Selecting DIB into memory DC.
            let old = unsafe { SelectObject(self.hdc, hbmp.into()) };
            self.hbmp = hbmp;
            self.old_obj = old;
            self.bits = bits.cast();
            self.cap_w = new_cw;
            self.cap_h = new_ch;
        }
    }

    fn free_dib(&mut self) {
        if !self.hbmp.is_invalid() {
            // SAFETY: Restoring previous GDI object and deleting DIB.
            unsafe {
                if !self.old_obj.is_invalid() {
                    let _ = SelectObject(self.hdc, self.old_obj);
                }
                let _ = DeleteObject(self.hbmp.into());
            }
            self.hbmp = HBITMAP::default();
            self.old_obj = HGDIOBJ::default();
            self.bits = std::ptr::null_mut();
        }
    }

    pub fn begin_draw(&self) -> bool {
        let rect = RECT {
            left: 0,
            top: 0,
            right: self.pixel_w,
            bottom: self.pixel_h,
        };
        // SAFETY: `self.rt` and `self.hdc` are valid and sized to `rect`.
        unsafe {
            if self.rt.BindDC(self.hdc, &rect).is_err() {
                return false;
            }
            self.rt.BeginDraw();
            self.rt.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }));
        }
        true
    }

    pub fn end_draw(&self) {
        // SAFETY: Matches `begin_draw`.
        unsafe {
            let _ = self.rt.EndDraw(None, None);
        }
    }

    pub fn mem_dc(&self) -> HDC {
        self.hdc
    }

    pub fn present_layered(&self, hwnd: HWND, x: i32, y: i32) {
        let pt_dst = POINT { x, y };
        let sz = SIZE {
            cx: self.pixel_w,
            cy: self.pixel_h,
        };
        let pt_src = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // SAFETY: `hwnd` and `self.hdc` are valid and sized to `sz`.
        unsafe {
            let _ = UpdateLayeredWindow(
                hwnd,
                None,
                Some(&pt_dst),
                Some(&sz),
                Some(self.hdc),
                Some(&pt_src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
        }
    }

    pub fn save_trimmed_png(&self, path: &Path) -> Result<(), String> {
        if self.bits.is_null() || self.pixel_w <= 0 || self.pixel_h <= 0 {
            return Err("empty surface".to_string());
        }
        let w = self.pixel_w as usize;
        let h = self.pixel_h as usize;
        let stride = self.cap_w as usize;
        // SAFETY: `self.bits` points to `stride * cap_h * 4` bytes of DIB memory.
        let bgra =
            unsafe { std::slice::from_raw_parts(self.bits, stride * (self.cap_h as usize) * 4) };

        let mut left = w;
        let mut top = h;
        let mut right: isize = -1;
        let mut bottom: isize = -1;
        for y in 0..h {
            for x in 0..w {
                let a = bgra[(y * stride + x) * 4 + 3];
                if a <= 2 {
                    continue;
                }
                left = left.min(x);
                right = right.max(x as isize);
                top = top.min(y);
                bottom = bottom.max(y as isize);
            }
        }
        let (x0, y0, cw, ch) = if right < 0 {
            (0, 0, w, h)
        } else {
            (
                left,
                top,
                (right as usize) - left + 1,
                (bottom as usize) - top + 1,
            )
        };

        let mut rgba = vec![0u8; cw * ch * 4];
        for row in 0..ch {
            for col in 0..cw {
                let si = ((y0 + row) * stride + (x0 + col)) * 4;
                let di = (row * cw + col) * 4;
                let b = bgra[si];
                let g = bgra[si + 1];
                let r = bgra[si + 2];
                let a = bgra[si + 3];
                if a == 0 {
                    rgba[di..di + 4].fill(0);
                } else if a == 255 {
                    rgba[di] = r;
                    rgba[di + 1] = g;
                    rgba[di + 2] = b;
                    rgba[di + 3] = 255;
                } else {
                    // Un-premultiply for standard PNG RGBA
                    let af = f32::from(a) / 255.0;
                    rgba[di] = (f32::from(r) / af).round().clamp(0.0, 255.0) as u8;
                    rgba[di + 1] = (f32::from(g) / af).round().clamp(0.0, 255.0) as u8;
                    rgba[di + 2] = (f32::from(b) / af).round().clamp(0.0, 255.0) as u8;
                    rgba[di + 3] = a;
                }
            }
        }

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), cw as u32, ch as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&rgba).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn brush(&self, c: Color) -> Option<ID2D1SolidColorBrush> {
        let d2d_c = D2D1_COLOR_F {
            r: c.r,
            g: c.g,
            b: c.b,
            a: c.a,
        };
        if let Some(ref b) = self.solid_brush {
            // SAFETY: `b` is a valid solid color brush owned by `self.rt`.
            unsafe { b.SetColor(&d2d_c) };
            return Some(b.clone());
        }
        // SAFETY: `self.rt` is valid.
        unsafe { self.rt.CreateSolidColorBrush(&d2d_c, None) }.ok()
    }

    pub fn fill_rect(&self, x: f32, y: f32, w: f32, h: f32, c: Color) {
        if c.a <= 0.001 || w <= 0.0 || h <= 0.0 {
            return;
        }
        if let Some(b) = self.brush(c) {
            let r = D2D_RECT_F {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            };
            // SAFETY: `self.rt` and `b` are valid.
            unsafe { self.rt.FillRectangle(&r, &b) };
        }
    }

    #[expect(clippy::too_many_arguments, reason = "Direct2D rounded rect helper")]
    pub fn fill_rounded_rect(&self, x: f32, y: f32, w: f32, h: f32, rx: f32, ry: f32, c: Color) {
        if c.a <= 0.001 || w <= 0.0 || h <= 0.0 {
            return;
        }
        if let Some(b) = self.brush(c) {
            let rr = D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F {
                    left: x,
                    top: y,
                    right: x + w,
                    bottom: y + h,
                },
                radiusX: rx,
                radiusY: ry,
            };
            // SAFETY: `self.rt` and `b` are valid.
            unsafe { self.rt.FillRoundedRectangle(&rr, &b) };
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Direct2D rounded rect stroke helper"
    )]
    pub fn draw_rounded_rect(
        &self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        rx: f32,
        ry: f32,
        c: Color,
        stroke_w: f32,
        stroke_style: Option<&ID2D1StrokeStyle>,
    ) {
        if c.a <= 0.001 || w <= 0.0 || h <= 0.0 {
            return;
        }
        if let Some(b) = self.brush(c) {
            let rr = D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F {
                    left: x,
                    top: y,
                    right: x + w,
                    bottom: y + h,
                },
                radiusX: rx,
                radiusY: ry,
            };
            // SAFETY: `self.rt` and `b` are valid.
            unsafe {
                self.rt
                    .DrawRoundedRectangle(&rr, &b, stroke_w, stroke_style)
            };
        }
    }

    pub fn fill_ellipse(&self, cx: f32, cy: f32, rx: f32, ry: f32, c: Color) {
        if c.a <= 0.001 || rx <= 0.0 || ry <= 0.0 {
            return;
        }
        if let Some(b) = self.brush(c) {
            let el = D2D1_ELLIPSE {
                point: Vector2::new(cx, cy),
                radiusX: rx,
                radiusY: ry,
            };
            // SAFETY: `self.rt` and `b` are valid.
            unsafe { self.rt.FillEllipse(&el, &b) };
        }
    }

    pub fn draw_ellipse(&self, cx: f32, cy: f32, rx: f32, ry: f32, c: Color, stroke_w: f32) {
        if c.a <= 0.001 || rx <= 0.0 || ry <= 0.0 {
            return;
        }
        if let Some(b) = self.brush(c) {
            let el = D2D1_ELLIPSE {
                point: Vector2::new(cx, cy),
                radiusX: rx,
                radiusY: ry,
            };
            // SAFETY: `self.rt` and `b` are valid.
            unsafe { self.rt.DrawEllipse(&el, &b, stroke_w, None) };
        }
    }

    pub fn fill_geometry(&self, geom: &ID2D1Geometry, c: Color) {
        if c.a <= 0.001 {
            return;
        }
        if let Some(b) = self.brush(c) {
            // SAFETY: `self.rt`, `geom`, and `b` are valid.
            unsafe { self.rt.FillGeometry(geom, &b, None) };
        }
    }

    pub fn draw_geometry(
        &self,
        geom: &ID2D1Geometry,
        c: Color,
        stroke_w: f32,
        stroke_style: Option<&ID2D1StrokeStyle>,
    ) {
        if c.a <= 0.001 {
            return;
        }
        if let Some(b) = self.brush(c) {
            // SAFETY: `self.rt`, `geom`, and `b` are valid.
            unsafe { self.rt.DrawGeometry(geom, &b, stroke_w, stroke_style) };
        }
    }

    #[expect(clippy::too_many_arguments, reason = "Direct2D line helper")]
    pub fn draw_line(
        &self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        c: Color,
        stroke_w: f32,
        stroke_style: Option<&ID2D1StrokeStyle>,
    ) {
        if c.a <= 0.001 {
            return;
        }
        if let Some(b) = self.brush(c) {
            // SAFETY: `self.rt` and `b` are valid.
            unsafe {
                self.rt.DrawLine(
                    Vector2::new(x1, y1),
                    Vector2::new(x2, y2),
                    &b,
                    stroke_w,
                    stroke_style,
                );
            }
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Direct2D quadratic Bezier curve helper"
    )]
    pub fn draw_quad_bezier(
        &self,
        factories: &D2dFactories,
        x0: f32,
        y0: f32,
        cx: f32,
        cy: f32,
        x1: f32,
        y1: f32,
        c: Color,
        stroke_w: f32,
    ) {
        if c.a <= 0.001 {
            return;
        }
        // SAFETY: `factories.d2d` is valid and sink is closed before drawing.
        let Ok(path) = (unsafe { factories.d2d.CreatePathGeometry() }) else {
            return;
        };
        // SAFETY: `path` is valid.
        let Ok(sink) = (unsafe { path.Open() }) else {
            return;
        };
        let c1x = x0 + (2.0 / 3.0) * (cx - x0);
        let c1y = y0 + (2.0 / 3.0) * (cy - y0);
        let c2x = x1 + (2.0 / 3.0) * (cx - x1);
        let c2y = y1 + (2.0 / 3.0) * (cy - y1);
        // SAFETY: Building single open cubic Bezier figure.
        unsafe {
            sink.BeginFigure(Vector2::new(x0, y0), D2D1_FIGURE_BEGIN_HOLLOW);
            sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                point1: Vector2::new(c1x, c1y),
                point2: Vector2::new(c2x, c2y),
                point3: Vector2::new(x1, y1),
            });
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
            if sink.Close().is_err() {
                return;
            }
        }
        let geom: ID2D1Geometry = path.cast().unwrap();
        self.draw_geometry(&geom, c, stroke_w, Some(&factories.round_stroke));
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Direct2D smooth Bezier curve with vertical gradient fill"
    )]
    pub fn draw_smooth_curve_with_fill(
        &self,
        factories: &D2dFactories,
        points: &[(f32, f32)],
        baseline_y: f32,
        stroke_color: Color,
        stroke_w: f32,
        fill_top: Color,
        fill_bottom: Color,
    ) {
        if points.len() < 2 {
            return;
        }
        let min_y = points
            .iter()
            .map(|p| p.1)
            .fold(f32::INFINITY, f32::min)
            .min(baseline_y - 1.0);

        // 1. Filled area under smooth curve
        // SAFETY: `factories.d2d` is valid and `fill_path`/`stroke_path` are newly created geometries.
        if let (Ok(fill_path), Ok(stroke_path)) = unsafe {
            (
                factories.d2d.CreatePathGeometry(),
                factories.d2d.CreatePathGeometry(),
            )
        } && let (Ok(fsink), Ok(ssink)) = unsafe { (fill_path.Open(), stroke_path.Open()) }
        {
            // SAFETY: Building closed fill path and open stroke path from `points`.
            unsafe {
                fsink.BeginFigure(
                    Vector2::new(points[0].0, baseline_y),
                    D2D1_FIGURE_BEGIN_FILLED,
                );
                fsink.AddLine(Vector2::new(points[0].0, points[0].1));
                ssink.BeginFigure(
                    Vector2::new(points[0].0, points[0].1),
                    D2D1_FIGURE_BEGIN_HOLLOW,
                );
                for w in points.windows(2) {
                    let p0 = w[0];
                    let p1 = w[1];
                    let mid_x = (p0.0 + p1.0) * 0.5;
                    let seg = D2D1_BEZIER_SEGMENT {
                        point1: Vector2::new(mid_x, p0.1),
                        point2: Vector2::new(mid_x, p1.1),
                        point3: Vector2::new(p1.0, p1.1),
                    };
                    fsink.AddBezier(&seg);
                    ssink.AddBezier(&seg);
                }
                let last = points[points.len() - 1];
                fsink.AddLine(Vector2::new(last.0, baseline_y));
                fsink.EndFigure(D2D1_FIGURE_END_CLOSED);
                ssink.EndFigure(D2D1_FIGURE_END_OPEN);
                let _ = fsink.Close();
                let _ = ssink.Close();
            }

            let stops = [
                D2D1_GRADIENT_STOP {
                    position: 0.0,
                    color: D2D1_COLOR_F {
                        r: fill_top.r,
                        g: fill_top.g,
                        b: fill_top.b,
                        a: fill_top.a,
                    },
                },
                D2D1_GRADIENT_STOP {
                    position: 1.0,
                    color: D2D1_COLOR_F {
                        r: fill_bottom.r,
                        g: fill_bottom.g,
                        b: fill_bottom.b,
                        a: fill_bottom.a,
                    },
                },
            ];
            // SAFETY: Creating vertical linear gradient brush for area fill.
            if let Ok(col) = unsafe {
                self.rt.CreateGradientStopCollection(
                    &stops,
                    windows::Win32::Graphics::Direct2D::D2D1_GAMMA_2_2,
                    D2D1_EXTEND_MODE_CLAMP,
                )
            } && let Ok(brush) = unsafe {
                self.rt.CreateLinearGradientBrush(
                    &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                        startPoint: Vector2::new(0.0, min_y),
                        endPoint: Vector2::new(0.0, baseline_y),
                    },
                    None,
                    &col,
                )
            } {
                let fgeom: ID2D1Geometry = fill_path.cast().unwrap();
                // SAFETY: `self.rt`, `fgeom`, and `brush` are valid.
                unsafe { self.rt.FillGeometry(&fgeom, &brush, None) };
            }

            let sgeom: ID2D1Geometry = stroke_path.cast().unwrap();
            self.draw_geometry(
                &sgeom,
                stroke_color.with_alpha(0.24),
                stroke_w * 2.8,
                Some(&factories.round_stroke),
            );
            self.draw_geometry(
                &sgeom,
                stroke_color,
                stroke_w,
                Some(&factories.round_stroke),
            );
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Direct2D sculpted squircle eye helper"
    )]
    pub fn fill_sculpted_eye(
        &self,
        factories: &D2dFactories,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        drop_l: f32,
        drop_r: f32,
        c: Color,
    ) {
        if c.a <= 0.001 || w <= 0.5 || h <= 0.5 {
            return;
        }
        let max_drop = drop_l.max(drop_r).clamp(0.0, (h - 1.0).max(0.0));
        let dl = drop_l.clamp(0.0, max_drop);
        let dr = drop_r.clamp(0.0, max_drop);
        let rad = r.min(w * 0.45).min(((h - dl).min(h - dr)) * 0.45).max(0.2);
        let k = 0.552_284_8_f32;
        let kr = rad * (1.0 - k);

        // SAFETY: `factories.d2d` is valid and sink is closed before drawing.
        let Ok(path) = (unsafe { factories.d2d.CreatePathGeometry() }) else {
            return;
        };
        // SAFETY: `path` is valid.
        let Ok(sink) = (unsafe { path.Open() }) else {
            return;
        };
        // SAFETY: Building closed sculpted squircle eye figure.
        unsafe {
            sink.BeginFigure(Vector2::new(x, y + dl + rad), D2D1_FIGURE_BEGIN_FILLED);
            sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                point1: Vector2::new(x, y + dl + kr),
                point2: Vector2::new(x + kr, y + dl),
                point3: Vector2::new(x + rad, y + dl),
            });
            sink.AddLine(Vector2::new(x + w - rad, y + dr));
            sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                point1: Vector2::new(x + w - kr, y + dr),
                point2: Vector2::new(x + w, y + dr + kr),
                point3: Vector2::new(x + w, y + dr + rad),
            });
            sink.AddLine(Vector2::new(x + w, y + h - rad));
            sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                point1: Vector2::new(x + w, y + h - kr),
                point2: Vector2::new(x + w - kr, y + h),
                point3: Vector2::new(x + w - rad, y + h),
            });
            sink.AddLine(Vector2::new(x + rad, y + h));
            sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                point1: Vector2::new(x + kr, y + h),
                point2: Vector2::new(x, y + h - kr),
                point3: Vector2::new(x, y + h - rad),
            });
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            if sink.Close().is_err() {
                return;
            }
        }
        let geom: ID2D1Geometry = path.cast().unwrap();
        self.fill_geometry(&geom, c);
    }

    pub fn fill_crescent_eye(
        &self,
        factories: &D2dFactories,
        cx: f32,
        cy: f32,
        w: f32,
        h: f32,
        c: Color,
    ) {
        if c.a <= 0.001 || w <= 0.5 || h <= 0.5 {
            return;
        }
        let hw = w * 0.5;
        // SAFETY: `factories.d2d` is valid and sink is closed before drawing.
        let Ok(path) = (unsafe { factories.d2d.CreatePathGeometry() }) else {
            return;
        };
        // SAFETY: `path` is valid.
        let Ok(sink) = (unsafe { path.Open() }) else {
            return;
        };
        // SAFETY: Building closed thick crescent eye figure.
        unsafe {
            sink.BeginFigure(
                Vector2::new(cx - hw, cy + h * 0.35),
                D2D1_FIGURE_BEGIN_FILLED,
            );
            sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                point1: Vector2::new(cx - hw * 0.55, cy - h * 0.65),
                point2: Vector2::new(cx + hw * 0.55, cy - h * 0.65),
                point3: Vector2::new(cx + hw, cy + h * 0.35),
            });
            sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                point1: Vector2::new(cx + hw * 0.95, cy + h * 0.65),
                point2: Vector2::new(cx + hw * 0.55, cy + h * 0.55),
                point3: Vector2::new(cx + hw * 0.45, cy + h * 0.35),
            });
            sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                point1: Vector2::new(cx + hw * 0.25, cy - h * 0.05),
                point2: Vector2::new(cx - hw * 0.25, cy - h * 0.05),
                point3: Vector2::new(cx - hw * 0.45, cy + h * 0.35),
            });
            sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                point1: Vector2::new(cx - hw * 0.55, cy + h * 0.55),
                point2: Vector2::new(cx - hw * 0.95, cy + h * 0.65),
                point3: Vector2::new(cx - hw, cy + h * 0.35),
            });
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            if sink.Close().is_err() {
                return;
            }
        }
        let geom: ID2D1Geometry = path.cast().unwrap();
        self.fill_geometry(&geom, c);
    }

    pub fn fill_sparkle(
        &self,
        factories: &D2dFactories,
        cx: f32,
        cy: f32,
        r: f32,
        rot_deg: f32,
        c: Color,
    ) {
        if c.a <= 0.001 || r <= 0.5 {
            return;
        }
        let rad = rot_deg.to_radians();
        let (sin_a, cos_a) = rad.sin_cos();
        let rot =
            |x: f32, y: f32| Vector2::new(cx + x * cos_a - y * sin_a, cy + x * sin_a + y * cos_a);
        let w = r * 0.22;
        // SAFETY: `factories.d2d` is valid and sink is closed before drawing.
        let Ok(path) = (unsafe { factories.d2d.CreatePathGeometry() }) else {
            return;
        };
        // SAFETY: `path` is valid.
        let Ok(sink) = (unsafe { path.Open() }) else {
            return;
        };
        // SAFETY: Building closed 4-point curved sparkle figure.
        unsafe {
            sink.BeginFigure(rot(0.0, -r), D2D1_FIGURE_BEGIN_FILLED);
            sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT {
                point1: rot(w, -w),
                point2: rot(r, 0.0),
            });
            sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT {
                point1: rot(w, w),
                point2: rot(0.0, r),
            });
            sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT {
                point1: rot(-w, w),
                point2: rot(-r, 0.0),
            });
            sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT {
                point1: rot(-w, -w),
                point2: rot(0.0, -r),
            });
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            if sink.Close().is_err() {
                return;
            }
        }
        let geom: ID2D1Geometry = path.cast().unwrap();
        self.fill_geometry(&geom, c);
    }

    #[expect(clippy::too_many_arguments, reason = "DirectWrite text draw helper")]
    pub fn draw_text(
        &self,
        factories: &D2dFactories,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        kind: FontKind,
        size: f32,
        semibold: bool,
        color: Color,
        halign: HAlign,
        valign: VAlign,
        tabular: bool,
        wrap: bool,
        ellipsis: bool,
        strikethrough: bool,
    ) {
        if text.is_empty() || color.a <= 0.001 || w <= 0.0 || h <= 0.0 {
            return;
        }
        let Ok(layout) = factories.create_text_layout(
            text,
            kind,
            size,
            semibold,
            w,
            h,
            halign,
            valign,
            tabular,
            wrap,
            ellipsis,
            strikethrough,
        ) else {
            return;
        };
        if let Some(b) = self.brush(color) {
            // SAFETY: `self.rt`, `layout`, and `b` are valid.
            unsafe {
                self.rt
                    .DrawTextLayout(Vector2::new(x, y), &layout, &b, Default::default());
            }
        }
    }

    pub fn set_transform(&self, m: Matrix3x2) {
        // SAFETY: `self.rt` is valid.
        unsafe { self.rt.SetTransform(&m) };
    }

    pub fn push_clip(&self, x: f32, y: f32, w: f32, h: f32) {
        let r = D2D_RECT_F {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        // SAFETY: `self.rt` is valid.
        unsafe {
            self.rt.PushAxisAlignedClip(
                &r,
                windows::Win32::Graphics::Direct2D::D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            );
        }
    }

    pub fn pop_clip(&self) {
        // SAFETY: Matches `push_clip`.
        unsafe { self.rt.PopAxisAlignedClip() };
    }

    #[expect(clippy::too_many_arguments, reason = "Direct2D layer push helper")]
    pub fn push_layer(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        opacity: f32,
        mask_geom: Option<&ID2D1Geometry>,
        mask_brush: Option<&ID2D1LinearGradientBrush>,
    ) {
        self.push_layer_masked(
            x,
            y,
            w,
            h,
            opacity,
            mask_geom,
            Matrix3x2::identity(),
            mask_brush,
        );
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Direct2D layer push helper with mask transform"
    )]
    pub fn push_layer_masked(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        opacity: f32,
        mask_geom: Option<&ID2D1Geometry>,
        mask_transform: Matrix3x2,
        mask_brush: Option<&ID2D1LinearGradientBrush>,
    ) {
        let layer = if let Some(l) = self.layer_pool.pop() {
            l
        } else {
            // SAFETY: `self.rt` is valid.
            let Ok(l) = (unsafe { self.rt.CreateLayer(None) }) else {
                return;
            };
            l
        };
        let params = D2D1_LAYER_PARAMETERS {
            contentBounds: D2D_RECT_F {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            },
            geometricMask: std::mem::ManuallyDrop::new(mask_geom.cloned()),
            maskAntialiasMode:
                windows::Win32::Graphics::Direct2D::D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            maskTransform: mask_transform,
            opacity,
            opacityBrush: std::mem::ManuallyDrop::new(mask_brush.map(|b| b.cast().unwrap())),
            layerOptions: D2D1_LAYER_OPTIONS_NONE,
        };
        // SAFETY: `self.rt` and `layer` are valid; ManuallyDrop fields are dropped cleanly after PushLayer.
        unsafe {
            self.rt.PushLayer(&params, &layer);
        }
        let _ = std::mem::ManuallyDrop::into_inner(params.geometricMask);
        let _ = std::mem::ManuallyDrop::into_inner(params.opacityBrush);
        self.active_layers.push(layer);
    }

    pub fn pop_layer(&mut self) {
        // SAFETY: Matches `push_layer`.
        unsafe { self.rt.PopLayer() };
        if let Some(layer) = self.active_layers.pop()
            && self.layer_pool.len() < 4
        {
            self.layer_pool.push(layer);
        }
    }

    pub fn create_radial_glow(
        &self,
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
        inner: Color,
        outer: Color,
    ) -> Option<ID2D1RadialGradientBrush> {
        if rx <= 0.5 || ry <= 0.5 || (inner.a <= 0.001 && outer.a <= 0.001) {
            return None;
        }
        let mid = inner.lerp(outer, 0.55).with_alpha(0.45);
        let stops = [
            D2D1_GRADIENT_STOP {
                position: 0.0,
                color: D2D1_COLOR_F {
                    r: inner.r,
                    g: inner.g,
                    b: inner.b,
                    a: inner.a,
                },
            },
            D2D1_GRADIENT_STOP {
                position: 0.5,
                color: D2D1_COLOR_F {
                    r: mid.r,
                    g: mid.g,
                    b: mid.b,
                    a: mid.a,
                },
            },
            D2D1_GRADIENT_STOP {
                position: 1.0,
                color: D2D1_COLOR_F {
                    r: outer.r,
                    g: outer.g,
                    b: outer.b,
                    a: outer.a,
                },
            },
        ];
        // SAFETY: `self.rt` is valid.
        unsafe {
            let col = self
                .rt
                .CreateGradientStopCollection(
                    &stops,
                    windows::Win32::Graphics::Direct2D::D2D1_GAMMA_2_2,
                    D2D1_EXTEND_MODE_CLAMP,
                )
                .ok()?;
            self.rt
                .CreateRadialGradientBrush(
                    &D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                        center: Vector2::new(cx, cy),
                        gradientOriginOffset: Vector2::new(0.0, 0.0),
                        radiusX: rx,
                        radiusY: ry,
                    },
                    None,
                    &col,
                )
                .ok()
        }
    }

    pub fn fill_radial_glow(&self, cx: f32, cy: f32, rx: f32, ry: f32, inner: Color, outer: Color) {
        if let Some(brush) = self.create_radial_glow(cx, cy, rx, ry, inner, outer) {
            let el = D2D1_ELLIPSE {
                point: Vector2::new(cx, cy),
                radiusX: rx,
                radiusY: ry,
            };
            // SAFETY: `self.rt` and `brush` are valid.
            unsafe { self.rt.FillEllipse(&el, &brush) };
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Direct2D rounded rect radial glow helper"
    )]
    pub fn fill_rounded_rect_radial_glow(
        &self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
        inner: Color,
        outer: Color,
    ) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        if let Some(brush) = self.create_radial_glow(cx, cy, rx, ry, inner, outer) {
            let rr = D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F {
                    left: x,
                    top: y,
                    right: x + w,
                    bottom: y + h,
                },
                radiusX: r,
                radiusY: r,
            };
            // SAFETY: `self.rt` and `brush` are valid.
            unsafe { self.rt.FillRoundedRectangle(&rr, &brush) };
        }
    }

    pub fn create_linear_mask(
        &self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        stops: &[(f32, f32)],
    ) -> Option<ID2D1LinearGradientBrush> {
        let d2d_stops: Vec<D2D1_GRADIENT_STOP> = stops
            .iter()
            .map(|&(pos, alpha)| D2D1_GRADIENT_STOP {
                position: pos,
                color: D2D1_COLOR_F {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: alpha,
                },
            })
            .collect();
        // SAFETY: `self.rt` is valid.
        unsafe {
            let col = self
                .rt
                .CreateGradientStopCollection(
                    &d2d_stops,
                    windows::Win32::Graphics::Direct2D::D2D1_GAMMA_2_2,
                    D2D1_EXTEND_MODE_CLAMP,
                )
                .ok()?;
            self.rt
                .CreateLinearGradientBrush(
                    &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                        startPoint: Vector2::new(x0, y0),
                        endPoint: Vector2::new(x1, y1),
                    },
                    None,
                    &col,
                )
                .ok()
        }
    }

    pub fn bitmap_from_bgra(&self, w: u32, h: u32, bgra_premul: &[u8]) -> Option<ID2D1Bitmap> {
        let props = D2D1_BITMAP_PROPERTIES {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
        };
        // SAFETY: `self.rt` is valid and `bgra_premul` has `w * h * 4` bytes.
        unsafe {
            self.rt
                .CreateBitmap(
                    D2D_SIZE_U {
                        width: w,
                        height: h,
                    },
                    Some(bgra_premul.as_ptr().cast()),
                    w * 4,
                    &props,
                )
                .ok()
        }
    }

    pub fn get_or_upload_image(&mut self, id: u64, frame: &DecodedFrame) -> Option<ID2D1Bitmap> {
        if let Some(bmp) = self.image_bitmaps.get(&id) {
            return Some(bmp.clone());
        }
        if self.image_bitmaps.len() >= 32 {
            self.image_bitmaps.clear();
        }
        let bmp = self.bitmap_from_bgra(frame.width, frame.height, &frame.bgra_premul)?;
        self.image_bitmaps.insert(id, bmp.clone());
        Some(bmp)
    }

    pub fn draw_bitmap(&self, bmp: &ID2D1Bitmap, x: f32, y: f32, w: f32, h: f32, opacity: f32) {
        let dst = D2D_RECT_F {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        // SAFETY: `self.rt` and `bmp` are valid.
        unsafe {
            self.rt.DrawBitmap(
                bmp,
                Some(&dst),
                opacity,
                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                None,
            );
        }
    }

    #[expect(clippy::too_many_arguments, reason = "9-slice soft shadow helper")]
    pub fn draw_soft_shadow(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        blur_radius: f64,
        depth: f64,
        opacity: f64,
    ) {
        let pad = blur_radius.ceil();
        let op_key = (opacity * 255.0).round().clamp(0.0, 255.0) as u8;
        let key = (pad as u32, op_key);
        if !self.shadow_cache.contains_key(&key)
            && let Some(slices) = self.build_shadow_slices(pad, opacity)
        {
            self.shadow_cache.insert(key, slices);
        }
        let Some(slices) = self.shadow_cache.get(&key).cloned() else {
            return;
        };
        let corner = (pad + 20.0) as f32;
        let outer_left = -(pad as f32);
        let outer_top = -(pad as f32) + (depth as f32);
        let outer_w = w + 2.0 * (pad as f32);
        let outer_h = h + 2.0 * (pad as f32);
        let cx = corner.min(outer_w / 2.0);
        let cy = corner.min(outer_h / 2.0);
        let xs = [
            outer_left,
            outer_left + cx,
            outer_left + outer_w - cx,
            outer_left + outer_w,
        ];
        let ys = [
            outer_top,
            outer_top + cy,
            outer_top + outer_h - cy,
            outer_top + outer_h,
        ];
        for row in 0..3 {
            for col in 0..3 {
                if row == 1 && col == 1 {
                    continue;
                }
                let dx = xs[col] + x;
                let dy = ys[row] + y;
                let dw = xs[col + 1] - xs[col];
                let dh = ys[row + 1] - ys[row];
                if dw > 0.0 && dh > 0.0 {
                    self.draw_bitmap(&slices[row * 3 + col], dx, dy, dw, dh, 1.0);
                }
            }
        }
    }

    fn build_shadow_slices(&self, pad: f64, opacity: f64) -> Option<[ID2D1Bitmap; 9]> {
        const CORNER: usize = 20;
        const MIDDLE: usize = 2;
        let p = pad as usize;
        let corner = p + CORNER;
        let size = 2 * corner + MIDDLE;
        let mut alpha = vec![0.0_f32; size * size];

        // Draw rounded rectangle alpha mask inside [p .. size - p]
        let r = CORNER as f32;
        let x0 = p as f32;
        let y0 = p as f32;
        let x1 = (size - p) as f32;
        let y1 = (size - p) as f32;
        for py in 0..size {
            for px in 0..size {
                let fx = px as f32 + 0.5;
                let fy = py as f32 + 0.5;
                let cx = fx.clamp(x0 + r, x1 - r);
                let cy = fy.clamp(y0 + r, y1 - r);
                let dx = fx - cx;
                let dy = fy - cy;
                let dist = (dx * dx + dy * dy).sqrt() - r;
                let cov = (0.5 - dist).clamp(0.0, 1.0);
                alpha[py * size + px] = cov * (opacity as f32);
            }
        }

        // Separable Gaussian blur matching WPF BlurEffect (sigma ~= pad / 2.5)
        let sigma = (pad as f32 / 2.5).max(1.0);
        let k_rad = p.max(1);
        let mut kernel = Vec::with_capacity(2 * k_rad + 1);
        let mut ksum = 0.0_f32;
        for i in -(k_rad as isize)..=(k_rad as isize) {
            let v = (-((i * i) as f32) / (2.0 * sigma * sigma)).exp();
            kernel.push(v);
            ksum += v;
        }
        for v in &mut kernel {
            *v /= ksum;
        }

        let mut tmp = vec![0.0_f32; size * size];
        for y in 0..size {
            for x in 0..size {
                let mut acc = 0.0_f32;
                for (ki, &kv) in kernel.iter().enumerate() {
                    let sx = x as isize + ki as isize - k_rad as isize;
                    if sx >= 0 && (sx as usize) < size {
                        acc += alpha[y * size + sx as usize] * kv;
                    }
                }
                tmp[y * size + x] = acc;
            }
        }
        let mut blurred = vec![0.0_f32; size * size];
        for y in 0..size {
            for x in 0..size {
                let mut acc = 0.0_f32;
                for (ki, &kv) in kernel.iter().enumerate() {
                    let sy = y as isize + ki as isize - k_rad as isize;
                    if sy >= 0 && (sy as usize) < size {
                        acc += tmp[sy as usize * size + x] * kv;
                    }
                }
                blurred[y * size + x] = acc;
            }
        }

        let starts = [0, corner, corner + MIDDLE];
        let lengths = [corner, MIDDLE, corner];
        let mut out: Vec<ID2D1Bitmap> = Vec::with_capacity(9);
        for row in 0..3 {
            for col in 0..3 {
                let sw = lengths[col];
                let sh = lengths[row];
                let mut bgra = vec![0u8; sw * sh * 4];
                for sy in 0..sh {
                    for sx in 0..sw {
                        let a = (blurred[(starts[row] + sy) * size + (starts[col] + sx)] * 255.0)
                            .round()
                            .clamp(0.0, 255.0) as u8;
                        bgra[(sy * sw + sx) * 4 + 3] = if a < 3 { 0 } else { a };
                    }
                }
                out.push(self.bitmap_from_bgra(sw as u32, sh as u32, &bgra)?);
            }
        }
        out.try_into().ok()
    }
}

impl Drop for D2dSurface {
    fn drop(&mut self) {
        self.free_dib();
        if !self.hdc.is_invalid() {
            // SAFETY: `self.hdc` was created by `CreateCompatibleDC`.
            unsafe {
                let _ = DeleteDC(self.hdc);
            }
        }
    }
}

fn build_pointer_geometry(d2d: &ID2D1Factory) -> windows_core::Result<ID2D1PathGeometry> {
    // SAFETY: `d2d` is valid.
    let path = unsafe { d2d.CreatePathGeometry()? };
    // SAFETY: `path` is valid and sink is closed before returning.
    let sink = unsafe { path.Open()? };
    // SAFETY: Building normalized 24x24 geometric cursor arrow polygon.
    unsafe {
        sink.BeginFigure(Vector2::new(5.0, 3.0), D2D1_FIGURE_BEGIN_FILLED);
        sink.AddLine(Vector2::new(20.0, 11.0));
        sink.AddLine(Vector2::new(13.0, 13.2));
        sink.AddLine(Vector2::new(10.5, 20.5));
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close()?;
    }
    Ok(path)
}
