/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Shared helpers for recreating the authentic iOS 5 (skeuomorphic) UIKit
//! appearance.
//!
//! iOS 5 controls are heavily *skeuomorphic*: bars and buttons are drawn with
//! a vertical two-part gloss gradient, a bright 1px highlight along the top
//! edge and a dark 1px shadow line along the bottom edge. This module provides
//! reusable primitives so the individual UIKit control implementations can
//! render that look consistently.
//!
//! The emulator's Core Graphics rasteriser only reliably rasterises axis
//! aligned rectangle fills and rectangle strokes (paths and real ellipses are
//! approximated). To stay within those guarantees, every gradient here is
//! synthesised as a stack of 1pt-tall rectangle "strips" whose colour is
//! linearly interpolated between the stops. This keeps the code fully
//! compatible with the existing renderer while still producing smooth
//! gradients on both Retina and non-Retina backing stores (the backing store
//! resolution is handled transparently by the context).

use crate::frameworks::core_graphics::cg_bitmap_context::{
    CGBitmapContextCreate, CGBitmapContextCreateImage,
};
use crate::frameworks::core_graphics::cg_color_space::CGColorSpaceCreateDeviceRGB;
use crate::frameworks::core_graphics::cg_context::{
    CGContextClearRect, CGContextFillRect, CGContextRef, CGContextRelease,
    CGContextRestoreGState, CGContextSaveGState, CGContextSetRGBFillColor,
};
use crate::frameworks::core_graphics::cg_image::{
    kCGImageAlphaPremultipliedLast, kCGImageByteOrder32Big,
};
use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::{NSInteger, NSUInteger};
use super::ui_control::UIControlEventTouchUpInside;
use crate::frameworks::uikit::ui_font::UITextAlignmentCenter;
use crate::mem::{GuestUSize, Ptr};
use crate::objc::{id, msg, msg_class, nil, release, SEL};
use crate::Environment;

/// A straight RGBA colour, components in the `0.0..=1.0` range.
pub type Rgba = (CGFloat, CGFloat, CGFloat, CGFloat);

#[inline]
fn lerp(a: CGFloat, b: CGFloat, t: CGFloat) -> CGFloat {
    a + (b - a) * t
}

#[inline]
fn lerp_rgba(a: Rgba, b: Rgba, t: CGFloat) -> Rgba {
    (
        lerp(a.0, b.0, t),
        lerp(a.1, b.1, t),
        lerp(a.2, b.2, t),
        lerp(a.3, b.3, t),
    )
}

/// Clamp a component to the representable colour range.
#[inline]
fn clamp01(x: CGFloat) -> CGFloat {
    x.clamp(0.0, 1.0)
}

/// Multiply the lightness of a colour by `factor` (values > 1 brighten,
/// < 1 darken). Alpha is preserved. Used to derive gloss stops from a single
/// bar tint colour.
#[inline]
pub fn scale_brightness(c: Rgba, factor: CGFloat) -> Rgba {
    (
        clamp01(c.0 * factor),
        clamp01(c.1 * factor),
        clamp01(c.2 * factor),
        c.3,
    )
}

/// Fill `rect` with a solid colour.
pub fn fill_solid(env: &mut Environment, ctx: CGContextRef, rect: CGRect, color: Rgba) {
    if ctx.is_null() || rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }
    CGContextSetRGBFillColor(env, ctx, color.0, color.1, color.2, color.3);
    CGContextFillRect(env, ctx, rect);
}

/// Fill `rect` with a smooth top-to-bottom linear gradient between `top` and
/// `bottom`. Rendered as a stack of 1pt strips (see the module docs).
pub fn fill_vertical_gradient(
    env: &mut Environment,
    ctx: CGContextRef,
    rect: CGRect,
    top: Rgba,
    bottom: Rgba,
) {
    if ctx.is_null() || rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }
    let steps = rect.size.height.ceil().max(1.0) as i32;
    for i in 0..steps {
        let t = if steps > 1 {
            i as CGFloat / (steps - 1) as CGFloat
        } else {
            0.0
        };
        let (r, g, b, a) = lerp_rgba(top, bottom, t);
        CGContextSetRGBFillColor(env, ctx, r, g, b, a);
        let strip = CGRect {
            origin: CGPoint {
                x: rect.origin.x,
                y: rect.origin.y + i as CGFloat,
            },
            size: CGSize {
                width: rect.size.width,
                // Slightly overlap so no seams appear between strips.
                height: 1.0,
            },
        };
        CGContextFillRect(env, ctx, strip);
    }
}

/// Draw a horizontal hairline (1pt tall by default) at `y` spanning the width
/// of `rect`. Used for the bright highlight / dark shadow edges of bars.
pub fn horizontal_line(
    env: &mut Environment,
    ctx: CGContextRef,
    rect: CGRect,
    y: CGFloat,
    color: Rgba,
    thickness: CGFloat,
) {
    fill_solid(
        env,
        ctx,
        CGRect {
            origin: CGPoint { x: rect.origin.x, y },
            size: CGSize {
                width: rect.size.width,
                height: thickness,
            },
        },
        color,
    );
}

/// The colour recipe for an iOS 5 bar (`UINavigationBar`, `UIToolbar`,
/// `UITabBar`). All values are hand-tuned to match iOS 5 screenshots.
#[derive(Clone, Copy)]
pub struct BarPalette {
    /// Bright highlight painted along the very top edge (1px).
    pub top_highlight: Rgba,
    /// Gradient stops for the upper (glossier) half of the bar.
    pub upper_top: Rgba,
    pub upper_bottom: Rgba,
    /// Gradient stops for the lower half of the bar.
    pub lower_top: Rgba,
    pub lower_bottom: Rgba,
    /// Dark shadow line painted along the very bottom edge (1px).
    pub bottom_shadow: Rgba,
}

impl BarPalette {
    /// The default blue-grey metallic tint used by `UINavigationBar` and
    /// `UIToolbar` on iOS 5.
    pub fn navigation_default() -> Self {
        BarPalette {
            top_highlight: (0.72, 0.76, 0.83, 1.0),
            upper_top: (0.56, 0.61, 0.70, 1.0),
            upper_bottom: (0.44, 0.51, 0.62, 1.0),
            lower_top: (0.40, 0.47, 0.59, 1.0),
            lower_bottom: (0.30, 0.37, 0.50, 1.0),
            bottom_shadow: (0.16, 0.20, 0.28, 1.0),
        }
    }

    /// `UIBarStyleBlack` bars (also the base for the `UITabBar`).
    pub fn black() -> Self {
        BarPalette {
            top_highlight: (0.34, 0.34, 0.36, 1.0),
            upper_top: (0.22, 0.22, 0.24, 1.0),
            upper_bottom: (0.13, 0.13, 0.15, 1.0),
            lower_top: (0.11, 0.11, 0.12, 1.0),
            lower_bottom: (0.02, 0.02, 0.03, 1.0),
            bottom_shadow: (0.0, 0.0, 0.0, 1.0),
        }
    }

    /// The light-grey gradient used behind a `UISearchBar` on iOS 5.
    pub fn search_bar() -> Self {
        BarPalette {
            top_highlight: (0.85, 0.86, 0.88, 1.0),
            upper_top: (0.74, 0.75, 0.78, 1.0),
            upper_bottom: (0.66, 0.68, 0.71, 1.0),
            lower_top: (0.62, 0.64, 0.67, 1.0),
            lower_bottom: (0.54, 0.56, 0.60, 1.0),
            bottom_shadow: (0.40, 0.42, 0.46, 1.0),
        }
    }

    /// The dark, glossy `UITabBar` background.
    pub fn tab_bar() -> Self {
        BarPalette {
            top_highlight: (0.40, 0.40, 0.42, 1.0),
            upper_top: (0.25, 0.25, 0.27, 1.0),
            upper_bottom: (0.15, 0.15, 0.16, 1.0),
            lower_top: (0.12, 0.12, 0.13, 1.0),
            lower_bottom: (0.03, 0.03, 0.04, 1.0),
            bottom_shadow: (0.0, 0.0, 0.0, 1.0),
        }
    }

    /// Derive a glossy palette from a single flat bar tint colour, mirroring
    /// how UIKit lightens/darkens `barTintColor`/`tintColor` to build the
    /// gradient.
    pub fn from_tint(tint: Rgba) -> Self {
        BarPalette {
            top_highlight: scale_brightness(tint, 1.45),
            upper_top: scale_brightness(tint, 1.18),
            upper_bottom: scale_brightness(tint, 1.02),
            lower_top: scale_brightness(tint, 0.92),
            lower_bottom: scale_brightness(tint, 0.72),
            bottom_shadow: scale_brightness(tint, 0.45),
        }
    }

    /// Apply an alpha multiplier to every stop (used for translucent bars).
    pub fn with_alpha(mut self, alpha: CGFloat) -> Self {
        for c in [
            &mut self.top_highlight,
            &mut self.upper_top,
            &mut self.upper_bottom,
            &mut self.lower_top,
            &mut self.lower_bottom,
            &mut self.bottom_shadow,
        ] {
            c.3 *= alpha;
        }
        self
    }
}

/// Render the full iOS 5 bar chrome (two-part gloss gradient plus top
/// highlight and bottom shadow lines) into `rect`.
pub fn draw_bar_background(
    env: &mut Environment,
    ctx: CGContextRef,
    rect: CGRect,
    palette: BarPalette,
) {
    if ctx.is_null() || rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }
    CGContextSaveGState(env, ctx);
    let mid = (rect.size.height / 2.0).floor();
    let upper = CGRect {
        origin: rect.origin,
        size: CGSize {
            width: rect.size.width,
            height: mid,
        },
    };
    let lower = CGRect {
        origin: CGPoint {
            x: rect.origin.x,
            y: rect.origin.y + mid,
        },
        size: CGSize {
            width: rect.size.width,
            height: rect.size.height - mid,
        },
    };
    fill_vertical_gradient(env, ctx, upper, palette.upper_top, palette.upper_bottom);
    fill_vertical_gradient(env, ctx, lower, palette.lower_top, palette.lower_bottom);
    // Bright highlight along the top edge.
    horizontal_line(env, ctx, rect, rect.origin.y, palette.top_highlight, 1.0);
    // Dark shadow along the bottom edge.
    horizontal_line(
        env,
        ctx,
        rect,
        rect.origin.y + rect.size.height - 1.0,
        palette.bottom_shadow,
        1.0,
    );
    CGContextRestoreGState(env, ctx);
}

/// Render an iOS 5 style glossy pill/button background inside `rect`. The
/// renderer cannot rasterise rounded corners into a bitmap, so callers that
/// need rounded corners should additionally set their layer's `cornerRadius`
/// (which the compositor rounds); this fills the glossy body.
pub fn draw_glossy_button(
    env: &mut Environment,
    ctx: CGContextRef,
    rect: CGRect,
    base: Rgba,
    highlighted: bool,
) {
    if ctx.is_null() || rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }
    CGContextSaveGState(env, ctx);
    let factor = if highlighted { 0.82 } else { 1.0 };
    let top = scale_brightness(base, 1.28 * factor);
    let upper_bottom = scale_brightness(base, 1.06 * factor);
    let lower_top = scale_brightness(base, 0.98 * factor);
    let bottom = scale_brightness(base, 0.80 * factor);
    let mid = (rect.size.height / 2.0).floor();
    let upper = CGRect {
        origin: rect.origin,
        size: CGSize {
            width: rect.size.width,
            height: mid,
        },
    };
    let lower = CGRect {
        origin: CGPoint {
            x: rect.origin.x,
            y: rect.origin.y + mid,
        },
        size: CGSize {
            width: rect.size.width,
            height: rect.size.height - mid,
        },
    };
    fill_vertical_gradient(env, ctx, upper, top, upper_bottom);
    fill_vertical_gradient(env, ctx, lower, lower_top, bottom);
    // Inner top highlight for the glass "shine".
    horizontal_line(
        env,
        ctx,
        rect,
        rect.origin.y,
        scale_brightness(base, 1.5 * factor),
        1.0,
    );
    CGContextRestoreGState(env, ctx);
}

/// Inset a rectangle by `dx`/`dy` on every edge.
#[inline]
pub fn inset_rect(rect: CGRect, dx: CGFloat, dy: CGFloat) -> CGRect {
    CGRect {
        origin: CGPoint {
            x: rect.origin.x + dx,
            y: rect.origin.y + dy,
        },
        size: CGSize {
            width: (rect.size.width - dx * 2.0).max(0.0),
            height: (rect.size.height - dy * 2.0).max(0.0),
        },
    }
}

/// Clear (make transparent) the four corner regions of `rect` so that content
/// already drawn into `rect` appears with rounded corners of the given
/// `radius`. The renderer cannot rasterise real arcs, so the arc is
/// approximated per scanline with horizontal clears — this looks smooth at
/// typical control sizes and works identically at Retina and non-Retina
/// backing-store resolutions.
///
/// This is how every rounded iOS 5 control in this theme obtains its rounded
/// silhouette without requiring the CoreAnimation compositor to clip drawn
/// layer `contents` to `cornerRadius`.
pub fn clear_rounded_corners(env: &mut Environment, ctx: CGContextRef, rect: CGRect, radius: CGFloat) {
    if ctx.is_null() {
        return;
    }
    let r = radius
        .min(rect.size.width / 2.0)
        .min(rect.size.height / 2.0);
    if r <= 0.5 {
        return;
    }
    let steps = r.ceil() as i32;
    let right_x = rect.origin.x + rect.size.width;
    let bottom_y = rect.origin.y + rect.size.height;
    for i in 0..steps {
        let y = i as CGFloat;
        // Horizontal distance from the arc centre for this scanline.
        let dy = r - y - 0.5;
        let inset = r - (r * r - dy * dy).max(0.0).sqrt();
        if inset <= 0.0 {
            continue;
        }
        // Top-left + top-right.
        CGContextClearRect(
            env,
            ctx,
            CGRect {
                origin: CGPoint {
                    x: rect.origin.x,
                    y: rect.origin.y + y,
                },
                size: CGSize {
                    width: inset,
                    height: 1.0,
                },
            },
        );
        CGContextClearRect(
            env,
            ctx,
            CGRect {
                origin: CGPoint {
                    x: right_x - inset,
                    y: rect.origin.y + y,
                },
                size: CGSize {
                    width: inset,
                    height: 1.0,
                },
            },
        );
        // Bottom-left + bottom-right.
        CGContextClearRect(
            env,
            ctx,
            CGRect {
                origin: CGPoint {
                    x: rect.origin.x,
                    y: bottom_y - y - 1.0,
                },
                size: CGSize {
                    width: inset,
                    height: 1.0,
                },
            },
        );
        CGContextClearRect(
            env,
            ctx,
            CGRect {
                origin: CGPoint {
                    x: right_x - inset,
                    y: bottom_y - y - 1.0,
                },
                size: CGSize {
                    width: inset,
                    height: 1.0,
                },
            },
        );
    }
}

/// Draw an iOS 5 glossy, rounded button/pill fully inside `rect`: a 1px darker
/// bezel, the glossy body and rounded corners. `base` is the button's fill
/// colour; pass `highlighted = true` for the pressed (darkened) state.
pub fn draw_rounded_glossy_button(
    env: &mut Environment,
    ctx: CGContextRef,
    rect: CGRect,
    base: Rgba,
    radius: CGFloat,
    highlighted: bool,
) {
    if ctx.is_null() || rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }
    CGContextSaveGState(env, ctx);
    // Darker bezel underneath.
    fill_solid(env, ctx, rect, scale_brightness(base, 0.5));
    // Glossy body inset by the 1px bezel.
    let body = inset_rect(rect, 1.0, 1.0);
    draw_glossy_button(env, ctx, body, base, highlighted);
    // Round the bezel, then round the (slightly smaller) body to leave a
    // 1px darker outline around the glossy face.
    clear_rounded_corners(env, ctx, rect, radius);
    CGContextRestoreGState(env, ctx);
}

/// Draw a recessed (inset) rounded track, as used by `UISlider`,
/// `UIProgressView` grooves and search fields. `fill` is the groove colour.
pub fn draw_recessed_track(
    env: &mut Environment,
    ctx: CGContextRef,
    rect: CGRect,
    fill: Rgba,
    radius: CGFloat,
) {
    if ctx.is_null() || rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }
    CGContextSaveGState(env, ctx);
    // Subtle darker top / lighter bottom to sell the "sunken" look.
    let top = scale_brightness(fill, 0.86);
    let bottom = scale_brightness(fill, 1.08);
    fill_vertical_gradient(env, ctx, rect, top, bottom);
    // Bright 1px bottom bevel highlight.
    horizontal_line(
        env,
        ctx,
        rect,
        rect.origin.y + rect.size.height - 1.0,
        (1.0, 1.0, 1.0, 0.35),
        1.0,
    );
    clear_rounded_corners(env, ctx, rect, radius);
    CGContextRestoreGState(env, ctx);
}

/// Palette + renderer for the iOS 5 dark, glossy rounded panel shared by
/// `UIAlertView` and `UIActionSheet`.
pub fn draw_alert_panel(env: &mut Environment, ctx: CGContextRef, rect: CGRect, radius: CGFloat) {
    if ctx.is_null() || rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }
    CGContextSaveGState(env, ctx);
    // Light outer stroke ("rim light") around the panel.
    fill_solid(env, ctx, rect, (0.72, 0.74, 0.78, 0.95));
    let body = inset_rect(rect, 1.0, 1.0);
    // Dark blue-charcoal glossy body: brighter glassy upper third, darker
    // lower body, matching the classic iOS 5 alert.
    let upper_h = (body.size.height * 0.5).floor();
    let upper = CGRect {
        origin: body.origin,
        size: CGSize {
            width: body.size.width,
            height: upper_h,
        },
    };
    let lower = CGRect {
        origin: CGPoint {
            x: body.origin.x,
            y: body.origin.y + upper_h,
        },
        size: CGSize {
            width: body.size.width,
            height: body.size.height - upper_h,
        },
    };
    fill_vertical_gradient(
        env,
        ctx,
        upper,
        (0.30, 0.34, 0.42, 0.98),
        (0.16, 0.19, 0.26, 0.98),
    );
    fill_vertical_gradient(
        env,
        ctx,
        lower,
        (0.13, 0.15, 0.21, 0.98),
        (0.07, 0.08, 0.12, 0.98),
    );
    // Glass shine on the very top.
    horizontal_line(env, ctx, body, body.origin.y, (0.5, 0.54, 0.62, 0.9), 1.0);
    clear_rounded_corners(env, ctx, rect, radius);
    CGContextRestoreGState(env, ctx);
}

/// Construct an `Rgba` from a `0xRRGGBB` literal (fully opaque).
pub fn rgb(hex: u32) -> Rgba {
    (
        ((hex >> 16) & 0xFF) as CGFloat / 255.0,
        ((hex >> 8) & 0xFF) as CGFloat / 255.0,
        (hex & 0xFF) as CGFloat / 255.0,
        1.0,
    )
}

/// Paint the subtle "linen" texture of an iOS 5 grouped table background on
/// top of the flat base colour the caller has already filled. The renderer
/// only rasterises axis-aligned rectangle fills, so the woven look is
/// synthesised as faint alternating 1pt horizontal lines plus a slight
/// darkening towards the top and bottom edges.
pub fn draw_grouped_texture(env: &mut Environment, ctx: CGContextRef, rect: CGRect) {
    if ctx.is_null() || rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }
    CGContextSaveGState(env, ctx);
    // Fine weave: every 3rd scanline gets a faint dark or bright tint.
    let steps = rect.size.height.ceil().max(1.0) as i32;
    for i in 0..steps {
        let (r, g, b, a) = match i % 3 {
            0 => (0.0, 0.0, 0.0, 0.025),
            1 => (1.0, 1.0, 1.0, 0.03),
            _ => continue,
        };
        CGContextSetRGBFillColor(env, ctx, r, g, b, a);
        CGContextFillRect(
            env,
            ctx,
            CGRect {
                origin: CGPoint {
                    x: rect.origin.x,
                    y: rect.origin.y + i as CGFloat,
                },
                size: CGSize {
                    width: rect.size.width,
                    height: 1.0,
                },
            },
        );
    }
    // Soft vignette at the top and bottom edges.
    let vignette = (0.0, 0.0, 0.0, 0.05);
    let band = CGSize {
        width: rect.size.width,
        height: 24.0,
    };
    fill_solid(
        env,
        ctx,
        CGRect {
            origin: rect.origin,
            size: band,
        },
        vignette,
    );
    fill_solid(
        env,
        ctx,
        CGRect {
            origin: CGPoint {
                x: rect.origin.x,
                y: rect.origin.y + rect.size.height - 24.0,
            },
            size: band,
        },
        vignette,
    );
    CGContextRestoreGState(env, ctx);
}

/// Sample a multi-stop gradient at `t` (`0.0..=1.0`).
fn sample_stops(stops: &[(CGFloat, Rgba)], t: CGFloat) -> Rgba {
    if stops.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    if stops.len() == 1 || t <= stops[0].0 {
        return stops[0].1;
    }
    let last = stops[stops.len() - 1];
    if t >= last.0 {
        return last.1;
    }
    for pair in stops.windows(2) {
        let (t0, c0) = pair[0];
        let (t1, c1) = pair[1];
        if t >= t0 && t <= t1 && t1 > t0 {
            return lerp_rgba(c0, c1, (t - t0) / (t1 - t0));
        }
    }
    last.1
}

/// Fill `rect` with a smooth top-to-bottom gradient defined by position/colour
/// `stops`. Like `fill_vertical_gradient`, rendered as a stack of 1pt strips.
fn fill_vertical_gradient_stops(
    env: &mut Environment,
    ctx: CGContextRef,
    rect: CGRect,
    stops: &[(CGFloat, Rgba)],
) {
    if ctx.is_null() || stops.is_empty() || rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }
    if stops.len() == 1 {
        fill_solid(env, ctx, rect, stops[0].1);
        return;
    }
    let steps = rect.size.height.ceil().max(1.0) as i32;
    for i in 0..steps {
        let t = if steps > 1 {
            i as CGFloat / (steps - 1) as CGFloat
        } else {
            0.0
        };
        let color = sample_stops(stops, t);
        CGContextSetRGBFillColor(env, ctx, color.0, color.1, color.2, color.3);
        let strip = CGRect {
            origin: CGPoint {
                x: rect.origin.x,
                y: rect.origin.y + i as CGFloat,
            },
            size: CGSize {
                width: rect.size.width,
                // Slightly overlap so no seams appear between strips.
                height: 1.0,
            },
        };
        CGContextFillRect(env, ctx, strip);
    }
}

/// The blue-grey metallic gradient used by `UISegmentedControl` on iOS 5.
pub const NAVIGATION: [(CGFloat, Rgba); 4] = [
    (0.0, (0.43, 0.53, 0.68, 1.0)),
    (0.5, (0.32, 0.42, 0.59, 1.0)),
    (0.9, (0.24, 0.34, 0.52, 1.0)),
    (1.0, (0.19, 0.28, 0.46, 1.0)),
];

/// Paint a rounded, gradient-filled surface (1px `border` ring, smooth
/// multi-stop gradient body, rounded corners) directly into `ctx`.
pub fn draw_surface(
    env: &mut Environment,
    ctx: CGContextRef,
    rect: CGRect,
    radius: CGFloat,
    stops: &[(CGFloat, Rgba)],
    border: Rgba,
) {
    if ctx.is_null() || stops.is_empty() || rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }
    CGContextSaveGState(env, ctx);
    // 1px border ring underneath the body.
    fill_solid(env, ctx, rect, border);
    let body = inset_rect(rect, 1.0, 1.0);
    fill_vertical_gradient_stops(env, ctx, body, stops);
    clear_rounded_corners(env, ctx, rect, radius);
    CGContextRestoreGState(env, ctx);
}

/// Give a whole view a themed gradient background: rasterise the surface into
/// an offscreen bitmap and install it as the view's layer `contents`, with
/// `cornerRadius` matching so the compositor keeps the rounded silhouette.
pub fn set_surface(
    env: &mut Environment,
    view: id,
    radius: CGFloat,
    stops: &[(CGFloat, Rgba)],
    border: Rgba,
) {
    if view.is_null() || stops.is_empty() {
        return;
    }
    let bounds: CGRect = msg![env; view bounds];
    let layer: id = msg![env; view layer];
    if layer.is_null() || bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
        return;
    }
    let width = bounds.size.width.round().max(1.0) as GuestUSize;
    let height = bounds.size.height.round().max(1.0) as GuestUSize;
    let color_space = CGColorSpaceCreateDeviceRGB(env);
    let ctx = CGBitmapContextCreate(
        env,
        Ptr::null(),
        width,
        height,
        8,
        width.checked_mul(4).unwrap(),
        color_space,
        kCGImageByteOrder32Big | kCGImageAlphaPremultipliedLast,
    );
    if ctx.is_null() {
        return;
    }
    draw_surface(
        env,
        ctx,
        CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize {
                width: width as CGFloat,
                height: height as CGFloat,
            },
        },
        radius,
        stops,
        border,
    );
    let image = CGBitmapContextCreateImage(env, ctx);
    // `setContents:` retains the new contents itself.
    () = msg![env; layer setContents:image];
    release(env, image);
    CGContextRelease(env, ctx);
    () = msg![env; layer setCornerRadius:radius];
}

/// Draw an iOS 5 navigation-bar button: a glossy rounded bezel in the bar's
/// blue tint. `back` marks a "back" button (slightly less rounded, with the
/// bezel meeting the screen edge).
pub fn draw_navigation_button(
    env: &mut Environment,
    ctx: CGContextRef,
    rect: CGRect,
    base: Rgba,
    back: bool,
    highlighted: bool,
) {
    if ctx.is_null() || rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }
    let radius = if back {
        3.0
    } else {
        (rect.size.height * 0.3).min(8.0)
    };
    draw_rounded_glossy_button(env, ctx, rect, base, radius, highlighted);
}

/// Corner radius of the shared `UIAlertView`/`UIActionSheet` panel.
const PANEL_RADIUS: CGFloat = 12.0;

/// Paint the dark glossy alert panel background into a bitmap and install it
/// as `view`'s layer contents.
fn set_panel_background(env: &mut Environment, view: id, radius: CGFloat) {
    let bounds: CGRect = msg![env; view bounds];
    let layer: id = msg![env; view layer];
    if layer.is_null() || bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
        return;
    }
    let width = bounds.size.width.round().max(1.0) as GuestUSize;
    let height = bounds.size.height.round().max(1.0) as GuestUSize;
    let color_space = CGColorSpaceCreateDeviceRGB(env);
    let ctx = CGBitmapContextCreate(
        env,
        Ptr::null(),
        width,
        height,
        8,
        width.checked_mul(4).unwrap(),
        color_space,
        kCGImageByteOrder32Big | kCGImageAlphaPremultipliedLast,
    );
    if ctx.is_null() {
        return;
    }
    draw_alert_panel(
        env,
        ctx,
        CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize {
                width: width as CGFloat,
                height: height as CGFloat,
            },
        },
        radius,
    );
    let image = CGBitmapContextCreateImage(env, ctx);
    // `setContents:` retains the new contents itself.
    () = msg![env; layer setContents:image];
    release(env, image);
    CGContextRelease(env, ctx);
    () = msg![env; layer setCornerRadius:radius];
}

/// Build and present the shared iOS 5 dark glossy panel used by
/// `UIAlertView` and `UIActionSheet`. `buttons` is an `NSArray` of title
/// strings; each button gets `index` as its tag and sends `action` to `this`
/// when tapped. `message` may be nil (action sheets have none). Returns the
/// overlay view (a plain `UIView` the caller retains as `host.overlay`), or
/// nil if the panel could not be presented.
pub fn present_panel(
    env: &mut Environment,
    this: id,
    action: SEL,
    title: id,
    message: id,
    buttons: id,
    destructive: NSInteger,
    sheet_style: bool,
) -> id {
    let app: id = msg_class![env; UIApplication sharedApplication];
    let window: id = msg![env; app keyWindow];
    if window == nil {
        return nil;
    }
    let screen_bounds: CGRect = msg![env; window bounds];

    let count: NSUInteger = if buttons == nil { 0 } else { msg![env; buttons count] };
    let mut titles = Vec::new();
    for i in 0..count {
        let button_title: id = msg![env; buttons objectAtIndex:i];
        titles.push(button_title);
    }

    // Panel geometry: centred alert or bottom-anchored action sheet.
    let panel_w = (screen_bounds.size.width - 16.0).min(284.0);
    let mut panel_h = if sheet_style {
        36.0 + count as CGFloat * 48.0
    } else {
        140.0
    };
    if !sheet_style && message != nil {
        panel_h += 12.0;
    }
    let panel_x;
    let panel_y;
    if sheet_style {
        panel_x = (screen_bounds.size.width - panel_w) / 2.0;
        panel_y = screen_bounds.origin.y + screen_bounds.size.height - panel_h - 8.0;
    } else {
        panel_x = screen_bounds.origin.x + (screen_bounds.size.width - panel_w) / 2.0;
        panel_y = screen_bounds.origin.y + (screen_bounds.size.height - panel_h) / 2.0 - 10.0;
    }
    let panel_rect = CGRect {
        origin: CGPoint {
            x: panel_x,
            y: panel_y,
        },
        size: CGSize {
            width: panel_w,
            height: panel_h,
        },
    };

    let overlay: id = msg_class![env; UIView alloc];
    let overlay: id = msg![env; overlay initWithFrame:panel_rect];
    if overlay == nil {
        return nil;
    }
    set_panel_background(env, overlay, PANEL_RADIUS);

    let white: id = msg_class![env; UIColor whiteColor];
    let clear: id = msg_class![env; UIColor clearColor];

    // Title.
    if title != nil {
        let label: id = msg_class![env; UILabel alloc];
        let title_rect = CGRect {
            origin: CGPoint { x: 12.0, y: 10.0 },
            size: CGSize { width: panel_w - 24.0, height: 22.0 },
        };
        let label: id = msg![env; label initWithFrame:title_rect];
        () = msg![env; label setText:title];
        let font: id = msg_class![env; UIFont boldSystemFontOfSize:17.0f32];
        () = msg![env; label setFont:font];
        () = msg![env; label setTextColor:white];
        () = msg![env; label setBackgroundColor:clear];
        () = msg![env; label setTextAlignment:UITextAlignmentCenter];
        () = msg![env; overlay addSubview:label];
        release(env, label);
    }

    // Message (alerts only).
    if message != nil {
        let label: id = msg_class![env; UILabel alloc];
        let message_rect = CGRect {
            origin: CGPoint { x: 12.0, y: 36.0 },
            size: CGSize { width: panel_w - 24.0, height: 36.0 },
        };
        let label: id = msg![env; label initWithFrame:message_rect];
        () = msg![env; label setText:message];
        let font: id = msg_class![env; UIFont systemFontOfSize:14.0f32];
        () = msg![env; label setFont:font];
        () = msg![env; label setTextColor:white];
        () = msg![env; label setBackgroundColor:clear];
        () = msg![env; label setTextAlignment:UITextAlignmentCenter];
        () = msg![env; overlay addSubview:label];
        release(env, label);
    }

    // Buttons.
    let font: id = msg_class![env; UIFont boldSystemFontOfSize:17.0f32];
    for (i, button_title) in titles.into_iter().enumerate() {
        let button: id = msg_class![env; UIButton buttonWithType:0i32];
        let frame = if sheet_style {
            CGRect {
                origin: CGPoint {
                    x: 8.0,
                    y: 36.0 + i as CGFloat * 48.0,
                },
                size: CGSize {
                    width: panel_w - 16.0,
                    height: 44.0,
                },
            }
        } else if count == 1 {
            CGRect {
                origin: CGPoint {
                    x: 12.0,
                    y: panel_h - 52.0,
                },
                size: CGSize {
                    width: panel_w - 24.0,
                    height: 40.0,
                },
            }
        } else if count == 2 {
            let button_w = (panel_w - 30.0) / 2.0;
            CGRect {
                origin: CGPoint {
                    x: 12.0 + i as CGFloat * (button_w + 6.0),
                    y: panel_h - 52.0,
                },
                size: CGSize {
                    width: button_w,
                    height: 40.0,
                },
            }
        } else {
            let button_h = ((panel_h - 84.0) / count as CGFloat).min(34.0);
            CGRect {
                origin: CGPoint {
                    x: 12.0,
                    y: 74.0 + i as CGFloat * (button_h + 2.0),
                },
                size: CGSize {
                    width: panel_w - 24.0,
                    height: button_h,
                },
            }
        };
        () = msg![env; button setFrame:frame];
        let tag: NSInteger = i as NSInteger;
        () = msg![env; button setTag:tag];
        () = msg![env; button setTitle:button_title forState:0u32];
        () = msg![env; button setTitleColor:white forState:0u32];
        let button_label: id = msg![env; button titleLabel];
        () = msg![env; button_label setFont:font];
        () = msg![env; button addTarget:this action:action forControlEvents:UIControlEventTouchUpInside];
        let (stops, border): ([(CGFloat, Rgba); 3], Rgba) = if i as NSInteger == destructive {
            (
                [
                    (0.0, (0.75, 0.28, 0.25, 1.0)),
                    (0.5, (0.62, 0.16, 0.14, 1.0)),
                    (1.0, (0.45, 0.10, 0.09, 1.0)),
                ],
                (0.35, 0.08, 0.07, 1.0),
            )
        } else {
            (
                [
                    (0.0, (0.55, 0.60, 0.68, 1.0)),
                    (0.5, (0.38, 0.44, 0.54, 1.0)),
                    (1.0, (0.24, 0.30, 0.40, 1.0)),
                ],
                (0.12, 0.15, 0.21, 1.0),
            )
        };
        set_surface(env, button, 6.0, &stops, border);
        () = msg![env; overlay addSubview:button];
    }

    () = msg![env; window addSubview:overlay];
    overlay
}