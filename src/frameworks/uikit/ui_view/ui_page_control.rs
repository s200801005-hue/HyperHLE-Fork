/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIPageControl`.

use super::ui_control::UIControlHostObject;
use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::NSInteger;
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes, release, retain,    ClassExports, NSZonePtr,
};

#[derive(Default)]
struct UIPageControlHostObject {
    superclass: UIControlHostObject,
    number_of_pages: NSInteger,
    current_page: NSInteger,
    hides_for_single_page: bool,
    defers_current_page_display: bool,
    /// UIColor* for the dots that are not current.
    page_indicator_tint_color: id,
    /// UIColor* for the current-page dot.
    current_page_indicator_tint_color: id,
}
impl_HostObject_with_superclass!(UIPageControlHostObject);

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIPageControl: UIControl

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(UIPageControlHostObject {
        superclass: UIControlHostObject::default(),
        number_of_pages: 0,
        current_page: 0,
        hides_for_single_page: false,
        defers_current_page_display: false,
        page_indicator_tint_color: nil,
        current_page_indicator_tint_color: nil,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

// MARK: - Size hint

// Returns the minimum size needed to display the dots for a given page count.
// Matches UIKit's fixed 7pt dot size with 9pt spacing.
+ (id)sizeForNumberOfPages:(NSInteger)page_count { // returns CGSize
    let width: f32 = (page_count.max(0) as f32) * 9.0 - 2.0; // 7pt dot + 2pt gap
    let size = crate::frameworks::core_graphics::CGSize {
        width: width.max(0.0),
        height: 36.0,
    };
    msg_class![env; NSValue valueWithCGSize:size]
}

- (id)init {
        msg_super![env; this init]
}

- (())layoutSubviews {
    () = msg_super![env; this layoutSubviews];
    () = msg![env; this setNeedsDisplay];
}

- (())drawRect:(CGRect)_rect {
    use super::ios5_theme::{draw_surface, rgb};
    let ctx = crate::frameworks::uikit::ui_graphics::UIGraphicsGetCurrentContext(env);
    let bounds: CGRect = msg![env; this bounds];
    let host = env.objc.borrow::<UIPageControlHostObject>(this);
    let (count, current, hidden, tint, selected_tint) = (
        host.number_of_pages.max(0), host.current_page, host.hides_for_single_page,
        host.page_indicator_tint_color, host.current_page_indicator_tint_color,
    );
    if hidden && count <= 1 { return; }
    let start = bounds.origin.x + (bounds.size.width - (count as f32 * 9.0 - 2.0)) / 2.0;
    for i in 0..count {
        let color = if i == current { selected_tint } else { tint };
        let color = if color != nil {
            crate::frameworks::uikit::ui_color::get_rgba(&env.objc, color)
        } else if i == current { rgb(0xFFFFFF) } else { rgb(0x9A9A9B) };
        let rect = CGRect {
            origin: CGPoint { x: start + i as f32 * 9.0, y: bounds.origin.y + (bounds.size.height - 7.0) / 2.0 },
            size: CGSize { width: 7.0, height: 7.0 },
        };
        draw_surface(env, ctx, rect, 3.5, &[(0.0, color), (1.0, color)], rgb(0x737374));
    }
}

- (())dealloc {
    let host = env.objc.borrow::<UIPageControlHostObject>(this);
    let (tint, current_tint) = (
        host.page_indicator_tint_color,
        host.current_page_indicator_tint_color,
    );
    release(env, tint);
    release(env, current_tint);
    msg_super![env; this dealloc]}

// MARK: - Page count

- (NSInteger)numberOfPages {
    env.objc.borrow::<UIPageControlHostObject>(this).number_of_pages
}

- (())setNumberOfPages:(NSInteger)number_of_pages {
    log_dbg!("UIPageControl setNumberOfPages:{}", number_of_pages);
    let host = env.objc.borrow_mut::<UIPageControlHostObject>(this);
    host.number_of_pages = number_of_pages;
    // Clamp current page to valid range.
    if number_of_pages == 0 {
        host.current_page = 0;
    } else if host.current_page >= number_of_pages {
        host.current_page = number_of_pages - 1;
    }
    () = msg![env; this setNeedsDisplay];   
}

// MARK: - Current page

- (NSInteger)currentPage {
    env.objc.borrow::<UIPageControlHostObject>(this).current_page
}

- (())setCurrentPage:(NSInteger)current_page {
    log_dbg!("UIPageControl setCurrentPage:{}", current_page);
    let n = env.objc.borrow::<UIPageControlHostObject>(this).number_of_pages;
    let clamped = if n == 0 {
        0
    } else {
        current_page.max(0).min(n - 1)
    };
    env.objc.borrow_mut::<UIPageControlHostObject>(this).current_page = clamped;
    () = msg![env; this setNeedsDisplay];    
}

// MARK: - Display options

- (bool)hidesForSinglePage {
    env.objc.borrow::<UIPageControlHostObject>(this).hides_for_single_page
}

- (())setHidesForSinglePage:(bool)hides {
    env.objc.borrow_mut::<UIPageControlHostObject>(this).hides_for_single_page = hides;
    () = msg![env; this setNeedsDisplay];    
}

- (bool)defersCurrentPageDisplay {
    env.objc.borrow::<UIPageControlHostObject>(this).defers_current_page_display
}

- (())setDefersCurrentPageDisplay:(bool)defers {
    env.objc.borrow_mut::<UIPageControlHostObject>(this).defers_current_page_display = defers;
}

// When defersCurrentPageDisplay is YES the app calls this to commit the
// pending page change to the display. We store it immediately either way.
- (())updateCurrentPageDisplay {
    () = msg![env; this setNeedsDisplay];}

// MARK: - Tint colors

- (id)pageIndicatorTintColor { // UIColor*
    env.objc.borrow::<UIPageControlHostObject>(this).page_indicator_tint_color
}

- (())setPageIndicatorTintColor:(id)color { // UIColor*
    let old = env.objc.borrow::<UIPageControlHostObject>(this).page_indicator_tint_color;
    release(env, old);
    retain(env, color);
    env.objc.borrow_mut::<UIPageControlHostObject>(this).page_indicator_tint_color = color;
    () = msg![env; this setNeedsDisplay];    
}

- (id)currentPageIndicatorTintColor { // UIColor*
    env.objc.borrow::<UIPageControlHostObject>(this).current_page_indicator_tint_color
}

- (())setCurrentPageIndicatorTintColor:(id)color { // UIColor*
    let old = env.objc.borrow::<UIPageControlHostObject>(this)
        .current_page_indicator_tint_color;
    release(env, old);
    retain(env, color);
    env.objc.borrow_mut::<UIPageControlHostObject>(this)
        .current_page_indicator_tint_color = color;
    () = msg![env; this setNeedsDisplay];        
}

// MARK: - Description

- (id)description {
    let (n, cur) = {
        let h = env.objc.borrow::<UIPageControlHostObject>(this);
        (h.number_of_pages, h.current_page)
    };
    let s = format!(
        "<UIPageControl: {:?}; numberOfPages={}; currentPage={}>",
        this, n, cur
    );
    let cstr = env.mem.alloc_and_write_cstr(s.as_bytes());
    msg_class![env; NSString stringWithUTF8String:cstr]
}

@end

};
