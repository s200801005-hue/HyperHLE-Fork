/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UINavigationBar`.

use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_string::get_static_str;
use crate::frameworks::uikit::{ui_color, ui_graphics::UIGraphicsGetCurrentContext};
use crate::frameworks::uikit::ui_view::ios5_theme::{self, BarPalette};
use crate::frameworks::foundation::NSInteger;
use crate::frameworks::uikit::ui_view::UIViewHostObject;
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes, release,
    retain, ClassExports, HostObject, NSZonePtr,
};

type UIBarStyle = NSInteger;
const UIBarStyleDefault: UIBarStyle = 0;
const UIBarStyleBlack: UIBarStyle = 1;
const UIBarStyleBlackOpaque: UIBarStyle = 2;
const UIBarStyleBlackTranslucent: UIBarStyle = 3;

#[derive(Default)]
struct UINavigationBarHostObject {
    /// Embedded `UIView` host state. `UINavigationBar` is a `UIView` subclass,
    /// so it must carry the superclass host object; without it every `UIView`
    /// method (frame, subviews, layout, ...) sent to a nav bar fails to borrow
    /// and falls back to a zeroed phantom.
    superclass: UIViewHostObject,
    /// UINavigationBarDelegate — weak reference
    delegate: id,
    bar_style: UIBarStyle,
    translucent: bool,
    tint_color: id,            // UIColor* — retained
    bar_tint_color: id,        // UIColor* — retained
    title_text_attributes: id, // NSDictionary* — retained
    /// NSMutableArray* of UINavigationItem* — retained
    items: id,
    /// UIImage* — retained
    shadow_image: id,
    /// UIImage* background — retained
    background_image: id,
    rendered_views: Vec<id>,
    rendered_buttons: Vec<(id, id, bool)>,
    laying_out: bool,
}
impl_HostObject_with_superclass!(UINavigationBarHostObject);

// MARK: - UINavigationItem

#[derive(Default)]
struct UINavigationItemHostObject {
    bar: id, // Weak owner, cleared when removed from the bar.
    title: id,        // NSString* — retained
    title_view: id,   // UIView* — retained
    prompt: id,       // NSString* — retained
    back_button: id,  // UIBarButtonItem* — retained
    left_button: id,  // UIBarButtonItem* — retained
    right_button: id, // UIBarButtonItem* — retained
    left_items: id,   // NSArray* — retained
    right_items: id,  // NSArray* — retained
    hides_back_button: bool,
    left_items_supplemented: bool,
}
impl HostObject for UINavigationItemHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UINavigationBar: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let items = msg_class![env; NSMutableArray new];
    let host_object = Box::new(UINavigationBarHostObject {
        superclass: Default::default(),
        delegate: nil,
        bar_style: UIBarStyleDefault,
        translucent: true,
        tint_color: nil,
        bar_tint_color: nil,
        title_text_attributes: nil,
        items,
        shadow_image: nil,
        background_image: nil,
        rendered_views: Vec::new(),
        rendered_buttons: Vec::new(),
        laying_out: false,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)init {
    // Run UIView's initializer so the backing layer and view state are set up.
    msg_super![env; this init]
}

- (id)initWithFrame:(CGRect)frame {
    msg_super![env; this initWithFrame:frame]
}

- (id)initWithCoder:(id)coder {
    msg_super![env; this initWithCoder:coder]
}

- (())dealloc {
    let host = env.objc.borrow::<UINavigationBarHostObject>(this);
    let (tint_color, bar_tint_color, title_attrs, items, shadow, bg) = (
        host.tint_color,
        host.bar_tint_color,
        host.title_text_attributes,
        host.items,
        host.shadow_image,
        host.background_image,
    );
    release(env, tint_color);
    release(env, bar_tint_color);
    release(env, title_attrs);
    let count: u32 = msg![env; items count];
    for i in 0..count {
        let item: id = msg![env; items objectAtIndex:i];
        env.objc.borrow_mut::<UINavigationItemHostObject>(item).bar = nil;
    }
    release(env, items);
    release(env, shadow);
    release(env, bg);
    // delegate is weak — no release
    msg_super![env; this dealloc]
}

// MARK: - Delegate

- (id)delegate {
    env.objc.borrow::<UINavigationBarHostObject>(this).delegate
}

- (())setDelegate:(id)delegate {
    // Weak reference — no retain.
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).delegate = delegate;
}

// MARK: - Style

- (UIBarStyle)barStyle {
    env.objc.borrow::<UINavigationBarHostObject>(this).bar_style
}

- (())setBarStyle:(UIBarStyle)style {
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).bar_style = style;
    () = msg![env; this setNeedsDisplay];
}

- (bool)isTranslucent {
    env.objc.borrow::<UINavigationBarHostObject>(this).translucent
}

- (())setTranslucent:(bool)value {
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).translucent = value;
}

// MARK: - Colors

- (id)tintColor {
    env.objc.borrow::<UINavigationBarHostObject>(this).tint_color
}

- (())setTintColor:(id)color {
    let old = env.objc.borrow::<UINavigationBarHostObject>(this).tint_color;
    release(env, old);
    retain(env, color);
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).tint_color = color;
    () = msg![env; this setNeedsDisplay];
}

- (id)barTintColor {
    env.objc.borrow::<UINavigationBarHostObject>(this).bar_tint_color
}

- (())setBarTintColor:(id)color {
    let old = env.objc.borrow::<UINavigationBarHostObject>(this).bar_tint_color;
    release(env, old);
    retain(env, color);
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).bar_tint_color = color;
}

// MARK: - Title attributes

- (id)titleTextAttributes {
    env.objc.borrow::<UINavigationBarHostObject>(this).title_text_attributes
}

- (())setTitleTextAttributes:(id)attrs {
    let old = env.objc.borrow::<UINavigationBarHostObject>(this).title_text_attributes;
    release(env, old);
    retain(env, attrs);
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).title_text_attributes = attrs;
    () = msg![env; this layoutSubviews];
}

// MARK: - Background / shadow images

- (id)shadowImage {
    env.objc.borrow::<UINavigationBarHostObject>(this).shadow_image
}

- (())setShadowImage:(id)image {
    let old = env.objc.borrow::<UINavigationBarHostObject>(this).shadow_image;
    release(env, old);
    retain(env, image);
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).shadow_image = image;
}

- (id)backgroundImageForBarMetrics:(NSInteger)_metrics {
    env.objc.borrow::<UINavigationBarHostObject>(this).background_image
}

- (())setBackgroundImage:(id)image forBarMetrics:(NSInteger)_metrics {
    let old = env.objc.borrow::<UINavigationBarHostObject>(this).background_image;
    release(env, old);
    retain(env, image);
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).background_image = image;
    () = msg![env; this setNeedsDisplay];
}

- (())setBackgroundImage:(id)image
         forBarPosition:(NSInteger)_position
             barMetrics:(NSInteger)_metrics {
    () = msg![env; this setBackgroundImage:image forBarMetrics:_metrics];
}

// MARK: - Items stack

- (id)items {
    env.objc.borrow::<UINavigationBarHostObject>(this).items
}

- (())setItems:(id)items { // NSArray*
    let old_items = env.objc.borrow::<UINavigationBarHostObject>(this).items;
    let new_items: id = msg_class![env; NSMutableArray new];
    let count: u32 = msg![env; items count];
    for i in 0..count {
        let item: id = msg![env; items objectAtIndex:i];
        () = msg![env; new_items addObject:item];
    }
    let count: u32 = msg![env; old_items count];
    for i in 0..count {
        let item: id = msg![env; old_items objectAtIndex:i];
        env.objc.borrow_mut::<UINavigationItemHostObject>(item).bar = nil;
    }
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).items = new_items;
    release(env, old_items);
    let count: u32 = msg![env; new_items count];
    for i in 0..count {
        let item: id = msg![env; new_items objectAtIndex:i];
        env.objc.borrow_mut::<UINavigationItemHostObject>(item).bar = this;
    }
    () = msg![env; this layoutSubviews];
}

- (())setItems:(id)items animated:(bool)_animated {
    () = msg![env; this setItems:items];
}

- (id)topItem {
    let items = env.objc.borrow::<UINavigationBarHostObject>(this).items;
    let count: u32 = msg![env; items count];
    if count == 0 { return nil; }
    msg![env; items objectAtIndex:(count - 1)]
}

- (id)backItem {
    let items = env.objc.borrow::<UINavigationBarHostObject>(this).items;
    let count: u32 = msg![env; items count];
    if count < 2 { return nil; }
    msg![env; items objectAtIndex:(count - 2)]
}

- (())pushNavigationItem:(id)item animated:(bool)_animated {
    let delegate = env.objc.borrow::<UINavigationBarHostObject>(this).delegate;
    // Ask delegate if we should push.
    if delegate != nil {
        let sel = env.objc.register_host_selector(
            "navigationBar:shouldPushItem:".to_string(), &mut env.mem,
        );
        let responds: bool = msg![env; delegate respondsToSelector:sel];
        if responds {
            let should: bool = msg![env; delegate navigationBar:this shouldPushItem:item];
            if !should { return; }
        }
    }
    let items = env.objc.borrow::<UINavigationBarHostObject>(this).items;
    () = msg![env; items addObject:item];
    env.objc.borrow_mut::<UINavigationItemHostObject>(item).bar = this;
    () = msg![env; this layoutSubviews];

    if delegate != nil {
        let sel = env.objc.register_host_selector(
            "navigationBar:didPushItem:".to_string(), &mut env.mem,
        );
        let responds: bool = msg![env; delegate respondsToSelector:sel];
        if responds {
            () = msg![env; delegate navigationBar:this didPushItem:item];
        }
    }
}

- (id)popNavigationItemAnimated:(bool)_animated {
    let items = env.objc.borrow::<UINavigationBarHostObject>(this).items;
    let count: u32 = msg![env; items count];
    if count == 0 { return nil; }

    let top: id = msg![env; items objectAtIndex:(count - 1)];

    let delegate = env.objc.borrow::<UINavigationBarHostObject>(this).delegate;
    if delegate != nil {
        let sel = env.objc.register_host_selector(
            "navigationBar:shouldPopItem:".to_string(), &mut env.mem,
        );
        let responds: bool = msg![env; delegate respondsToSelector:sel];
        if responds {
            let should: bool = msg![env; delegate navigationBar:this shouldPopItem:top];
            if !should { return nil; }
        }
    }

    retain(env, top);
    env.objc.borrow_mut::<UINavigationItemHostObject>(top).bar = nil;
    () = msg![env; items removeLastObject];
    () = msg![env; this layoutSubviews];

    if delegate != nil {
        let sel = env.objc.register_host_selector(
            "navigationBar:didPopItem:".to_string(), &mut env.mem,
        );
        let responds: bool = msg![env; delegate respondsToSelector:sel];
        if responds {
            () = msg![env; delegate navigationBar:this didPopItem:top];
        }
    }
    crate::objc::autorelease(env, top)
}

- (())drawRect:(CGRect)_rect {
    let ctx = UIGraphicsGetCurrentContext(env);
    if ctx == nil { return; }
    let bounds: CGRect = msg![env; this bounds];
    let host = env.objc.borrow::<UINavigationBarHostObject>(this);
    let (image, tint, style, buttons) = (
        host.background_image, host.tint_color, host.bar_style,
        host.rendered_buttons.clone(),
    );
    let base = if tint != nil { ui_color::get_rgba(&env.objc, tint) }
        else if style != UIBarStyleDefault { (0.20, 0.20, 0.21, 1.0) }
        else { (0.36, 0.46, 0.62, 1.0) };
    if image != nil {
        () = msg![env; image drawInRect:bounds];
    } else {
        let palette = if tint != nil { BarPalette::from_tint(base) }
            else if style != UIBarStyleDefault { BarPalette::black() }
            else { BarPalette::navigation_default() };
        ios5_theme::draw_bar_background(env, ctx, bounds, palette);
    }
    for (button, _, back) in buttons {
        let frame: CGRect = msg![env; button frame];
        let highlighted: bool = msg![env; button isHighlighted];
        ios5_theme::draw_navigation_button(env, ctx, frame, base, back, highlighted);
    }
}

- (())_touchHLEButtonStateChanged:(id)_sender {
    () = msg![env; this setNeedsDisplay];
}

- (())_touchHLEActivateButton:(id)sender {
    retain(env, sender);
    let entry = env.objc.borrow::<UINavigationBarHostObject>(this)
        .rendered_buttons.iter().find(|(button, _, _)| *button == sender).copied();
    if let Some((_, item, back)) = entry {
        if back {
            let _: id = msg![env; this popNavigationItemAnimated:true];
        } else if item != nil {
            retain(env, item);
            let action: crate::objc::SEL = msg![env; item action];
            if !action.is_null() {
                let mut target: id = msg![env; item target];
                if target == nil {
                    let delegate: id = msg![env; this delegate];
                    let sel = env.objc.register_host_selector("topViewController".into(), &mut env.mem);
                    let responds: bool = msg![env; delegate respondsToSelector:sel];
                    target = if responds { msg![env; delegate topViewController] } else { this };
                    while target != nil {
                        let responds: bool = msg![env; target respondsToSelector:action];
                        if responds { break; }
                        target = msg![env; target nextResponder];
                    }
                }
                if target != nil {
                    let app: id = msg_class![env; UIApplication sharedApplication];
                    let _: bool = msg![env; app sendAction:action to:target from:item forEvent:nil];
                }
            }
            release(env, item);
        }
    }
    () = msg![env; this setNeedsDisplay];
    crate::objc::autorelease(env, sender);
}

- (())setFrame:(CGRect)frame {
    () = msg_super![env; this setFrame:frame];
    () = msg![env; this layoutSubviews];
}

- (())layoutSubviews {
    let host = env.objc.borrow_mut::<UINavigationBarHostObject>(this);
    if host.laying_out { return; }
    host.laying_out = true;
    host.rendered_buttons.clear();
    let old_views = std::mem::take(&mut host.rendered_views);
    for view in old_views {
        () = msg![env; view removeFromSuperview];
    }
    let bounds: CGRect = msg![env; this bounds];
    let top: id = msg![env; this topItem];
    if top != nil && bounds.size.width > 0.0 && bounds.size.height > 0.0 {
        let left: id = msg![env; top leftBarButtonItem];
        let right: id = msg![env; top rightBarButtonItem];
        let previous: id = msg![env; this backItem];
        let hidden: bool = msg![env; top hidesBackButton];
        let back = left == nil && previous != nil && !hidden;
        let left_item: id = if back { msg![env; previous backBarButtonItem] } else { left };
        let mut left_title: id = msg![env; left_item title];
        if back && left_title == nil { left_title = msg![env; previous title]; }
        if back && left_title == nil { left_title = get_static_str(env, "Back"); }
        let left_width: f32 = if left != nil || back {
            msg![env; this _touchHLELayoutItem:left_item title:left_title back:back right:false]
        } else { 0.0 };
        let right_title: id = msg![env; right title];
        let right_width: f32 = if right != nil {
            msg![env; this _touchHLELayoutItem:right title:right_title back:false right:true]
        } else { 0.0 };
        let margin = left_width.max(right_width) + 12.0;
        let rect = CGRect {
            origin: CGPoint { x: bounds.origin.x + margin, y: bounds.origin.y },
            size: CGSize { width: (bounds.size.width - margin * 2.0).max(0.0), height: bounds.size.height },
        };
        let custom: id = msg![env; top titleView];
        let title_view: id = if custom != nil { custom } else {
            let label: id = msg_class![env; UILabel alloc];
            let label: id = msg![env; label initWithFrame:rect];
            let title: id = msg![env; top title];
            let font: id = msg_class![env; UIFont boldSystemFontOfSize:20.0f32];
            let white: id = msg_class![env; UIColor whiteColor];
            let shadow: id = msg_class![env; UIColor colorWithWhite:0.0f32 alpha:0.6f32];
            let clear: id = msg_class![env; UIColor clearColor];
            () = msg![env; label setText:title];
            () = msg![env; label setFont:font];
            () = msg![env; label setTextColor:white];
            () = msg![env; label setShadowColor:shadow];
            () = msg![env; label setShadowOffset:(CGSize { width: 0.0, height: -1.0 })];
            () = msg![env; label setTextAlignment:1i32];
            () = msg![env; label setBackgroundColor:clear];
            () = msg![env; label setUserInteractionEnabled:false];
            let attrs = env.objc.borrow::<UINavigationBarHostObject>(this).title_text_attributes;
            if attrs != nil {
                let key = get_static_str(env, "UITextAttributeFont");
                let value: id = msg![env; attrs objectForKey:key];
                if value != nil { () = msg![env; label setFont:value]; }
                let key = get_static_str(env, "UITextAttributeTextColor");
                let value: id = msg![env; attrs objectForKey:key];
                if value != nil { () = msg![env; label setTextColor:value]; }
                let key = get_static_str(env, "UITextAttributeTextShadowColor");
                let value: id = msg![env; attrs objectForKey:key];
                if value != nil { () = msg![env; label setShadowColor:value]; }
            }
            label
        };
        () = msg![env; title_view setFrame:rect];
        () = msg![env; this addSubview:title_view];
        env.objc.borrow_mut::<UINavigationBarHostObject>(this).rendered_views.push(title_view);
        if custom == nil { release(env, title_view); }
    }
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).laying_out = false;
    () = msg![env; this setNeedsDisplay];
}

- (f32)_touchHLELayoutItem:(id)item title:(id)title back:(bool)back right:(bool)right {
    let bounds: CGRect = msg![env; this bounds];
    let custom: id = msg![env; item customView];
    let font: id = msg_class![env; UIFont boldSystemFontOfSize:12.0f32];
    let text_size: CGSize = msg![env; title sizeWithFont:font];
    let explicit_width: f32 = msg![env; item width];
    let width = if custom != nil {
        let frame: CGRect = msg![env; custom frame];
        frame.size.width
    } else if explicit_width > 0.0 { explicit_width }
    else { (text_size.width + if back { 28.0 } else { 20.0 }).max(34.0) };
    let width = width.min((bounds.size.width * 0.4).max(0.0));
    let height = (bounds.size.height - 14.0).clamp(0.0, 30.0);
    let frame = CGRect {
        origin: CGPoint {
            x: bounds.origin.x + if right { bounds.size.width - width - 6.0 } else { 6.0 },
            y: bounds.origin.y + (bounds.size.height - height) / 2.0,
        },
        size: CGSize { width, height },
    };
    if custom != nil {
        () = msg![env; custom setFrame:frame];
        () = msg![env; this addSubview:custom];
        env.objc.borrow_mut::<UINavigationBarHostObject>(this).rendered_views.push(custom);
        return width;
    }
    let button: id = msg_class![env; UIButton buttonWithType:0i32];
    let white: id = msg_class![env; UIColor whiteColor];
    let shadow: id = msg_class![env; UIColor colorWithWhite:0.0f32 alpha:0.65f32];
    () = msg![env; button setFrame:frame];
    () = msg![env; button setTitle:title forState:0u32];
    () = msg![env; button setTitleColor:white forState:0u32];
    let label: id = msg![env; button titleLabel];
    () = msg![env; label setFont:font];
    () = msg![env; label setShadowColor:shadow];
    () = msg![env; label setShadowOffset:(CGSize { width: 0.0, height: -1.0 })];
    if item != nil {
        let image: id = msg![env; item image];
        () = msg![env; button setImage:image forState:0u32];
        let enabled: bool = msg![env; item isEnabled];
        () = msg![env; button setEnabled:enabled];
    }
    let action = env.objc.register_host_selector("_touchHLEActivateButton:".into(), &mut env.mem);
    () = msg![env; button addTarget:this action:action forControlEvents:64u32];
    let state = env.objc.register_host_selector("_touchHLEButtonStateChanged:".into(), &mut env.mem);
    () = msg![env; button addTarget:this action:state forControlEvents:401u32];
    () = msg![env; this addSubview:button];
    let host = env.objc.borrow_mut::<UINavigationBarHostObject>(this);
    host.rendered_views.push(button);
    host.rendered_buttons.push((button, item, back));
    width
}

// MARK: - Size

- (CGSize)sizeThatFits:(CGSize)_size {
    // Standard navigation bar height.
    CGSize { width: 320.0, height: 44.0 }
}

@end

@implementation UINavigationItem: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(UINavigationItemHostObject {
        bar: nil,
        title: nil,
        title_view: nil,
        prompt: nil,
        back_button: nil,
        left_button: nil,
        right_button: nil,
        left_items: nil,
        right_items: nil,
        hides_back_button: false,
        left_items_supplemented: false,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithTitle:(id)title {
    retain(env, title);
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).title = title;
    this
}

- (id)init {
    this
}

// --- ДОБАВЛЕНО: Поддержка загрузки из NIB-файла ---
- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this init];
    let key = get_static_str(env, "UITitle");
    let title: id = msg![env; coder decodeObjectForKey:key];
    () = msg![env; this setTitle:title];
    let key = get_static_str(env, "UITitleView");
    let view: id = msg![env; coder decodeObjectForKey:key];
    () = msg![env; this setTitleView:view];
    let key = get_static_str(env, "UIBackBarButtonItem");
    let item: id = msg![env; coder decodeObjectForKey:key];
    () = msg![env; this setBackBarButtonItem:item];
    let key = get_static_str(env, "UILeftBarButtonItem");
    let item: id = msg![env; coder decodeObjectForKey:key];
    () = msg![env; this setLeftBarButtonItem:item];
    let key = get_static_str(env, "UIRightBarButtonItem");
    let item: id = msg![env; coder decodeObjectForKey:key];
    () = msg![env; this setRightBarButtonItem:item];
    let key = get_static_str(env, "UIHidesBackButton");
    let hidden: bool = msg![env; coder decodeBoolForKey:key];
    () = msg![env; this setHidesBackButton:hidden];
    this
}
// --------------------------------------------------

- (())dealloc {
    let host = env.objc.borrow::<UINavigationItemHostObject>(this);
    let (title, title_view, prompt, back, left, right, left_items, right_items) = (
        host.title, host.title_view, host.prompt,
        host.back_button, host.left_button, host.right_button,
        host.left_items, host.right_items,
    );
    release(env, title);
    release(env, title_view);
    release(env, prompt);
    release(env, back);
    release(env, left);
    release(env, right);
    release(env, left_items);
    release(env, right_items);
    env.objc.dealloc_object(this, &mut env.mem)
}

- (id)title {
    env.objc.borrow::<UINavigationItemHostObject>(this).title
}

- (())setTitle:(id)title {
    let old = env.objc.borrow::<UINavigationItemHostObject>(this).title;
    release(env, old);
    retain(env, title);
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).title = title;
    let bar = env.objc.borrow::<UINavigationItemHostObject>(this).bar;
    () = msg![env; bar layoutSubviews];
}

- (id)titleView {
    env.objc.borrow::<UINavigationItemHostObject>(this).title_view
}

- (())setTitleView:(id)view {
    let old = env.objc.borrow::<UINavigationItemHostObject>(this).title_view;
    release(env, old);
    retain(env, view);
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).title_view = view;
    let bar = env.objc.borrow::<UINavigationItemHostObject>(this).bar;
    () = msg![env; bar layoutSubviews];
}

- (id)prompt {
    env.objc.borrow::<UINavigationItemHostObject>(this).prompt
}

- (())setPrompt:(id)prompt {
    let old = env.objc.borrow::<UINavigationItemHostObject>(this).prompt;
    release(env, old);
    retain(env, prompt);
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).prompt = prompt;
}

- (id)backBarButtonItem {
    env.objc.borrow::<UINavigationItemHostObject>(this).back_button
}

- (())setBackBarButtonItem:(id)item {
    let old = env.objc.borrow::<UINavigationItemHostObject>(this).back_button;
    release(env, old);
    retain(env, item);
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).back_button = item;
    let bar = env.objc.borrow::<UINavigationItemHostObject>(this).bar;
    () = msg![env; bar layoutSubviews];
}

- (id)leftBarButtonItem {
    env.objc.borrow::<UINavigationItemHostObject>(this).left_button
}

- (())setLeftBarButtonItem:(id)item {
    let old = env.objc.borrow::<UINavigationItemHostObject>(this).left_button;
    release(env, old);
    retain(env, item);
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).left_button = item;
    let bar = env.objc.borrow::<UINavigationItemHostObject>(this).bar;
    () = msg![env; bar layoutSubviews];
}

- (())setLeftBarButtonItem:(id)item animated:(bool)_animated {
    () = msg![env; this setLeftBarButtonItem:item];
}

- (id)rightBarButtonItem {
    env.objc.borrow::<UINavigationItemHostObject>(this).right_button
}

- (())setRightBarButtonItem:(id)item {
    let old = env.objc.borrow::<UINavigationItemHostObject>(this).right_button;
    release(env, old);
    retain(env, item);
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).right_button = item;
    let bar = env.objc.borrow::<UINavigationItemHostObject>(this).bar;
    () = msg![env; bar layoutSubviews];
}

- (())setRightBarButtonItem:(id)item animated:(bool)_animated {
    () = msg![env; this setRightBarButtonItem:item];
}

- (id)leftBarButtonItems {
    env.objc.borrow::<UINavigationItemHostObject>(this).left_items
}

- (())setLeftBarButtonItems:(id)items {
    let old = env.objc.borrow::<UINavigationItemHostObject>(this).left_items;
    release(env, old);
    retain(env, items);
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).left_items = items;
    // Sync single-item accessor to first element.
    if items != nil {
        let count: u32 = msg![env; items count];
        let first: id = if count > 0 { msg![env; items objectAtIndex:0u32] } else { nil };
        () = msg![env; this setLeftBarButtonItem:first];
    }
}

- (id)rightBarButtonItems {
    env.objc.borrow::<UINavigationItemHostObject>(this).right_items
}

- (())setRightBarButtonItems:(id)items {
    let old = env.objc.borrow::<UINavigationItemHostObject>(this).right_items;
    release(env, old);
    retain(env, items);
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).right_items = items;
    if items != nil {
        let count: u32 = msg![env; items count];
        let first: id = if count > 0 { msg![env; items objectAtIndex:0u32] } else { nil };
        () = msg![env; this setRightBarButtonItem:first];
    }
}

- (bool)hidesBackButton {
    env.objc.borrow::<UINavigationItemHostObject>(this).hides_back_button
}

- (())setHidesBackButton:(bool)value {
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).hides_back_button = value;
    let bar = env.objc.borrow::<UINavigationItemHostObject>(this).bar;
    () = msg![env; bar layoutSubviews];
}

- (())setHidesBackButton:(bool)value animated:(bool)_animated {
    () = msg![env; this setHidesBackButton:value];
}

- (bool)leftItemsSupplementBackButton {
    env.objc.borrow::<UINavigationItemHostObject>(this).left_items_supplemented
}

- (())setLeftItemsSupplementBackButton:(bool)value {
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).left_items_supplemented = value;
}

@end

};