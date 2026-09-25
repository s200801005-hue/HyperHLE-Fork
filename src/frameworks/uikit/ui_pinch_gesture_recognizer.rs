/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIPinchGestureRecognizer`.
//!
//! The base class `UIGestureRecognizer` is implemented in
//! `ui_gesture_recognizer.rs`. This module only provides the
//! `UIPinchGestureRecognizer` subclass with its `scale` and `velocity`
//! properties.
//!
//! Apple documentation:
//! - <https://developer.apple.com/documentation/uikit/uipinchgesturerecognizer>

use super::ui_gesture_recognizer::UIGestureRecognizerHostObject;
use crate::objc::{id, impl_HostObject_with_superclass, objc_classes, ClassExports, NSZonePtr};

// MARK: - UIPinchGestureRecognizer host object
#[derive(Default)]
struct UIPinchGestureRecognizerHostObject {
    superclass: UIGestureRecognizerHostObject, 
    scale: CGFloat,
    velocity: CGFloat,
}
impl_HostObject_with_superclass!(UIPinchGestureRecognizerHostObject);

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

// =========================================================================
// MARK: - UIPinchGestureRecognizer
// =========================================================================

@implementation UIPinchGestureRecognizer: UIGestureRecognizer

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(UIPinchGestureRecognizerHostObject {
         superclass: Default::default(),
        scale: 1.0,
        velocity: 0.0,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

// MARK: - Properties

- (CGFloat)scale {
    env.objc.borrow::<UIPinchGestureRecognizerHostObject>(this).scale
}

- (())setScale:(CGFloat)scale {
    env.objc.borrow_mut::<UIPinchGestureRecognizerHostObject>(this).scale = scale;
}

- (CGFloat)velocity {
    env.objc.borrow::<UIPinchGestureRecognizerHostObject>(this).velocity
}

@end

};
