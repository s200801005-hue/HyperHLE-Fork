/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIActionSheet`.

use crate::frameworks::foundation::NSUInteger;
use crate::frameworks::core_graphics::CGRect;
use crate::frameworks::uikit::ui_view::{ios5_theme, UIViewHostObject};
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil,
    objc_classes, release, retain, ClassExports, NSZonePtr,};

#[derive(Default)]
struct UIActionSheetHostObject {
    superclass: UIViewHostObject,
    overlay: id,    
    delegate: id,
    title: id,
    /// NSMutableArray* of NSString* button titles
    button_titles: id,
    cancel_button_index: i32,
    destructive_button_index: i32,
    /// Tag for app use
    tag: i32,
    visible: bool,
    action_sheet_style: i32,
}
impl_HostObject_with_superclass!(UIActionSheetHostObject);
pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIActionSheet: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(UIActionSheetHostObject {
        superclass: UIViewHostObject::default(),
        overlay: nil,        
        delegate: nil,
        title: nil,
        button_titles: nil,
        cancel_button_index: -1,
        destructive_button_index: -1,
        tag: 0,
        visible: false,
        action_sheet_style: -1,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

// MARK: - Designated initializer

- (id)initWithTitle:(id)title               // NSString*
           delegate:(id)delegate
  cancelButtonTitle:(id)cancel_title        // NSString*
destructiveButtonTitle:(id)destructive_title // NSString*
  otherButtonTitles:(id)other_titles {      // NSString*, ..., nil  (varargs stub)
    // Initialise button array.
    let buttons: id = msg_class![env; NSMutableArray new];
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).button_titles = buttons;

    // Set title.
    retain(env, title);
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).title = title;

    // Set delegate.
    retain(env, delegate);
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).delegate = delegate;

    // Add destructive button first (index 0 when present), matching UIKit.
    if destructive_title != nil {
        let idx: NSUInteger = msg![env; buttons count];
        let _: () = msg![env; buttons addObject:destructive_title];
        env.objc.borrow_mut::<UIActionSheetHostObject>(this).destructive_button_index =
            idx as i32;
    }

    // Add the first "other" button if provided (varargs not supported here;
    // callers wanting more buttons should use addButtonWithTitle: afterwards).
    if other_titles != nil {
        let _: () = msg![env; buttons addObject:other_titles];
    }

    // Add cancel button last, matching UIKit behaviour.
    if cancel_title != nil {
        let idx: NSUInteger = msg![env; buttons count];
        let _: () = msg![env; buttons addObject:cancel_title];
        env.objc.borrow_mut::<UIActionSheetHostObject>(this).cancel_button_index =
            idx as i32;
    }

    this
}

- (())dealloc {
    let host = env.objc.borrow::<UIActionSheetHostObject>(this);
    let (delegate, title, button_titles) =
        (host.delegate, host.title, host.button_titles);
    release(env, delegate);
    release(env, title);
    release(env, button_titles);
    msg_super![env; this dealloc]}

// MARK: - Delegate

- (id)delegate {
    env.objc.borrow::<UIActionSheetHostObject>(this).delegate
}

- (())setDelegate:(id)delegate {
    let old = env.objc.borrow::<UIActionSheetHostObject>(this).delegate;
    release(env, old);
    retain(env, delegate);
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).delegate = delegate;
}

// MARK: - Title

- (id)title {
    env.objc.borrow::<UIActionSheetHostObject>(this).title
}

- (())setTitle:(id)title {
    let old = env.objc.borrow::<UIActionSheetHostObject>(this).title;
    release(env, old);
    retain(env, title);
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).title = title;
}

// MARK: - Tag

- (i32)tag {
    env.objc.borrow::<UIActionSheetHostObject>(this).tag
}

- (())setTag:(i32)tag {
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).tag = tag;
}

// MARK: - Buttons

- (NSUInteger)addButtonWithTitle:(id)title { // NSString* -> NSInteger (button index)
    let buttons = env.objc.borrow::<UIActionSheetHostObject>(this).button_titles;
    let idx: NSUInteger = msg![env; buttons count];
    let _: () = msg![env; buttons addObject:title];
    idx
}

- (NSUInteger)numberOfButtons {
    let buttons = env.objc.borrow::<UIActionSheetHostObject>(this).button_titles;
    msg![env; buttons count]
}

- (i32)actionSheetStyle {
    env.objc.borrow::<UIActionSheetHostObject>(this).action_sheet_style
}

- (())setActionSheetStyle:(i32)style {
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).action_sheet_style = style;
}

- (id)buttonTitleAtIndex:(NSUInteger)index { // NSString*
    let buttons = env.objc.borrow::<UIActionSheetHostObject>(this).button_titles;
    let count: NSUInteger = msg![env; buttons count];
    if index >= count {
        return nil;
    }
    msg![env; buttons objectAtIndex:index]
}

// MARK: - Special button indices

- (i32)cancelButtonIndex {
    env.objc.borrow::<UIActionSheetHostObject>(this).cancel_button_index
}

- (())setCancelButtonIndex:(i32)index {
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).cancel_button_index = index;
}

- (i32)destructiveButtonIndex {
    env.objc.borrow::<UIActionSheetHostObject>(this).destructive_button_index
}

- (())setDestructiveButtonIndex:(i32)index {
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).destructive_button_index = index;
}

- (i32)firstOtherButtonIndex {
    // First button that is neither cancel nor destructive.
    let host = env.objc.borrow::<UIActionSheetHostObject>(this);
    let buttons = host.button_titles;
    let cancel = host.cancel_button_index;
    let destructive = host.destructive_button_index;
    let count: NSUInteger = msg![env; buttons count];
    for i in 0..count {
        let i_signed = i as i32;
        if i_signed != cancel && i_signed != destructive {
            return i_signed;
        }
    }
    -1
}

// MARK: - Visibility

- (bool)isVisible {
    env.objc.borrow::<UIActionSheetHostObject>(this).visible
}

// MARK: - Presentation
// touchHLE has no sheet UI. We immediately fire the cancel callback so the
// app's delegate can clean up, matching the MPMediaPickerController pattern.

- (())showInView:(id)_view {
        if env.objc.borrow::<UIActionSheetHostObject>(this).visible { return; }
    let host = env.objc.borrow::<UIActionSheetHostObject>(this);
    let (title, buttons, destructive) = (host.title, host.button_titles, host.destructive_button_index);
    let action = env.objc.register_host_selector("_touchHLEPanelButton:".into(), &mut env.mem);
    let overlay = ios5_theme::present_panel(env, this, action, title, nil, buttons, destructive, true);
    if overlay == nil { return; }
    retain(env, this);
    let host = env.objc.borrow_mut::<UIActionSheetHostObject>(this);
    host.overlay = overlay;
    host.visible = true;
}

- (())_touchHLEPanelButton:(id)sender {
    let index: i32 = msg![env; sender tag];
    () = msg![env; this dismissWithClickedButtonIndex:(index as NSUInteger) animated:false];
}

- (())showFromToolbar:(id)toolbar {
    () = msg![env; this showInView:toolbar];
}

- (())showFromTabBar:(id)tab_bar {
    () = msg![env; this showInView:tab_bar];
}

- (())showFromBarButtonItem:(id)_item animated:(bool)_animated {
    () = msg![env; this showInView:nil];}

- (())showFromRect:(CGRect)_rect inView:(id)view animated:(bool)_animated {
    () = msg![env; this showInView:view];}

// MARK: - Dismissal

- (())dismissWithClickedButtonIndex:(NSUInteger)index animated:(bool)_animated {
    let host = env.objc.borrow_mut::<UIActionSheetHostObject>(this);
    if !host.visible { return; }
    host.visible = false;
    let overlay = std::mem::replace(&mut host.overlay, nil);
    let delegate = host.delegate;
    () = msg![env; overlay removeFromSuperview];
    release(env, overlay);
    if delegate == nil {
        release(env, this);
        return;
    }

    let sel_clicked        = env.objc.register_host_selector("actionSheet:clickedButtonAtIndex:".to_string(), &mut env.mem);
    let sel_will_dismiss   = env.objc.register_host_selector("actionSheet:willDismissWithButtonIndex:".to_string(), &mut env.mem);
    let sel_did_dismiss    = env.objc.register_host_selector("actionSheet:didDismissWithButtonIndex:".to_string(), &mut env.mem);

    let responds_clicked: bool      = msg![env; delegate respondsToSelector:sel_clicked];
    let responds_will:    bool      = msg![env; delegate respondsToSelector:sel_will_dismiss];
    let responds_did:     bool      = msg![env; delegate respondsToSelector:sel_did_dismiss];

    if responds_clicked {
        let _: () = msg![env; delegate actionSheet:this clickedButtonAtIndex:index];
    }
    if responds_will {
        let _: () = msg![env; delegate actionSheet:this willDismissWithButtonIndex:index];
    }
    if responds_did {
        let _: () = msg![env; delegate actionSheet:this didDismissWithButtonIndex:index];
    }
    release(env, this);
}

- (())dismissDidClickedButtonIndex:(NSUInteger)index {
    () = msg![env; this dismissWithClickedButtonIndex:index animated:false];
}

// Private helper — dismiss via cancel button (or index 0 as fallback).
- (())_touchHLE_dismiss {
    if !env.objc.borrow::<UIActionSheetHostObject>(this).visible { return; }
    let delegate = env.objc.borrow::<UIActionSheetHostObject>(this).delegate;
    if delegate != nil {
        let sel_cancel = env.objc.register_host_selector("actionSheetCancel:".to_string(), &mut env.mem);
        let responds: bool = msg![env; delegate respondsToSelector:sel_cancel];
        if responds {
            let _: () = msg![env; delegate actionSheetCancel:this];
        }
    }

    let cancel_index = env.objc.borrow::<UIActionSheetHostObject>(this).cancel_button_index;
    let dismiss_index: NSUInteger = if cancel_index >= 0 {
        cancel_index as NSUInteger
    } else {
        0
    };
    let _: () = msg![env; this dismissDidClickedButtonIndex:dismiss_index];
}

@end

};
