/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIPickerView`.

use crate::frameworks::foundation::NSUInteger;
use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};
use crate::frameworks::uikit::ui_view::ios5_theme;
use crate::frameworks::uikit::ui_view::UIViewHostObject;
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes, release, retain,
    ClassExports, NSZonePtr,
};

use std::collections::HashMap;
use std::time::Instant;

struct WheelAnimation {
    from: f32,
    to: f32,
    started: Instant,
    duration: f32,
    notify: bool,
}

struct WheelDrag {
    touch: id,
    component: NSUInteger,
    start_y: f32,
    last_y: f32,
    timestamp: f64,
    raw_position: f32,
    velocity: f32,
    moved: bool,
}

fn release_target(position: f32, velocity: f32, last: f32) -> f32 {
    (position + velocity.clamp(-50.0, 50.0) * 0.2).round().clamp(0.0, last)
}

fn animation_position(from: f32, to: f32, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    from + (to - from) * (1.0 - (1.0 - t).powi(3))
}

fn rubber_band(position: f32, last: f32) -> f32 {
    let clamped = position.clamp(0.0, last);
    let excess = position - clamped;
    clamped + excess / (1.0 + excess.abs() * 2.0)
}

#[derive(Default)]
struct UIPickerViewHostObject {
    superclass: UIViewHostObject,
    delegate: id,
    data_source: id,
    shows_selection_indicator: bool,
    /// Cached component count (from data source).
    number_of_components: NSUInteger,
    selected_rows: HashMap<NSUInteger, NSUInteger>,
    positions: HashMap<NSUInteger, f32>,
    animations: HashMap<NSUInteger, WheelAnimation>,
    drag: Option<WheelDrag>,
    timer: id,
    row_views: HashMap<(NSUInteger, NSUInteger), id>,
    row_containers: HashMap<NSUInteger, id>,
    row_indicators: HashMap<NSUInteger, id>,
    rows_revision: u64,
    updating_rows: bool,
}
impl_HostObject_with_superclass!(UIPickerViewHostObject);

fn clear_row_views(env: &mut crate::Environment, picker: id, component: Option<NSUInteger>) {
    let host = env.objc.borrow_mut::<UIPickerViewHostObject>(picker);
    let was_updating = std::mem::replace(&mut host.updating_rows, true);
    host.rows_revision = host.rows_revision.wrapping_add(1);
    let keys: Vec<_> = host.row_views.keys().copied()
        .filter(|&(c, _)| component.is_none_or(|wanted| c == wanted)).collect();
    let views: Vec<_> = keys.into_iter().filter_map(|key| host.row_views.remove(&key)).collect();
    let keys: Vec<_> = host.row_containers.keys().copied()
        .filter(|&c| component.is_none_or(|wanted| c == wanted)).collect();
    let containers: Vec<_> = keys.into_iter().filter_map(|key| host.row_containers.remove(&key)).collect();
    let keys: Vec<_> = host.row_indicators.keys().copied()
        .filter(|&c| component.is_none_or(|wanted| c == wanted)).collect();
    let indicators: Vec<_> = keys.into_iter().filter_map(|key| host.row_indicators.remove(&key)).collect();
    for view in views.into_iter().chain(indicators).chain(containers) {
        () = msg![env; view removeFromSuperview];
        release(env, view);
    }
    env.objc.borrow_mut::<UIPickerViewHostObject>(picker).updating_rows = was_updating;
}

fn row_views_current(env: &crate::Environment, picker: id, revision: u64) -> bool {
    env.objc.borrow::<UIPickerViewHostObject>(picker).rows_revision == revision
}

fn update_row_views(env: &mut crate::Environment, picker: id, delegate: id, revision: u64) {
    let sel = env.objc.register_host_selector(
        "pickerView:viewForRow:forComponent:reusingView:".into(), &mut env.mem);
    let custom: bool = msg![env; delegate respondsToSelector:sel];
    if !custom || !row_views_current(env, picker, revision) { return; }
    let count = env.objc.borrow::<UIPickerViewHostObject>(picker).number_of_components;
    for component in 0..count {
        if !row_views_current(env, picker, revision) { break; }
        update_component_rows(env, picker, delegate, revision, component);
    }
}

fn obtain_row_view(
    env: &mut crate::Environment, picker: id, delegate: id, revision: u64,
    component: NSUInteger, row: NSUInteger, reusable: &mut Vec<id>,
) -> id {
    let existing = env.objc.borrow::<UIPickerViewHostObject>(picker).row_views
        .get(&(component, row)).copied().unwrap_or(nil);
    if existing != nil {
        retain(env, existing);
        return existing;
    }
    let reused = reusable.pop().unwrap_or(nil);
    let view: id = msg![env; delegate pickerView:picker viewForRow:row
        forComponent:component reusingView:reused];
    retain(env, view);
    release(env, reused);
    if !row_views_current(env, picker, revision) {
        release(env, view);
        return nil;
    }
    let host = env.objc.borrow::<UIPickerViewHostObject>(picker);
    // One UIView cannot represent two visible rows or contain itself.
    if view == nil || view == picker || host.row_views.values().any(|&v| v == view)
        || host.row_containers.values().any(|&v| v == view) {
        release(env, view);
        return nil;
    }
    retain(env, view);
    env.objc.borrow_mut::<UIPickerViewHostObject>(picker).row_views.insert((component, row), view);
    view
}

fn update_component_rows(
    env: &mut crate::Environment, picker: id, delegate: id, revision: u64, component: NSUInteger,
) {
    let frame: CGRect = msg![env; picker _touchHLEFrameForComponent:component];
    if !row_views_current(env, picker, revision) { return; }
    let size: CGSize = msg![env; picker rowSizeForComponent:component];
    if !row_views_current(env, picker, revision) { return; }
    let count: NSUInteger = msg![env; picker numberOfRowsInComponent:component];
    if !row_views_current(env, picker, revision) { return; }
    let wheel = ios5_theme::inset_rect(frame, 4.0, 8.0);
    if !size.height.is_finite() || size.height <= 0.0 { return; }
    let host = env.objc.borrow::<UIPickerViewHostObject>(picker);
    let selected = host.selected_rows.get(&component).copied().unwrap_or(0)
        .min(count.saturating_sub(1));
    let position = host.positions.get(&component).copied().unwrap_or(selected as f32);
    let mut container = host.row_containers.get(&component).copied().unwrap_or(nil);
    if container == nil {
        container = msg_class![env; UIView alloc];
        container = msg![env; container initWithFrame:wheel];
        () = msg![env; container setOpaque:false];
        () = msg![env; container setUserInteractionEnabled:false];
        () = msg![env; container setClipsToBounds:true];
        let layer: id = msg![env; container layer];
        () = msg![env; layer setMasksToBounds:true];
        env.objc.borrow_mut::<UIPickerViewHostObject>(picker).row_containers.insert(component, container);
        () = msg![env; picker addSubview:container];
    }
    // Keep it alive across delegate callbacks that can reload the picker.
    retain(env, container);
    () = msg![env; container setFrame:wheel];
    let (first, end) = visible_rows(position, wheel.size.height, size.height, count);
    let host = env.objc.borrow_mut::<UIPickerViewHostObject>(picker);
    let obsolete: Vec<_> = host.row_views.keys().copied().filter(|&(c, r)|
        c == component && (r < first || r >= end)).collect();
    let mut reusable: Vec<_> = obsolete.into_iter()
        .filter_map(|key| host.row_views.remove(&key)).collect();
    for &view in &reusable { () = msg![env; view removeFromSuperview]; }
    for row in first..end {
        if !row_views_current(env, picker, revision) { break; }
        let view = obtain_row_view(env, picker, delegate, revision, component, row, &mut reusable);
        if view == nil { continue; }
        if row_views_current(env, picker, revision) {
            let bounds: CGRect = msg![env; view bounds];
            let width = if bounds.size.width > 0.0 { bounds.size.width } else { wheel.size.width };
            let height = if bounds.size.height > 0.0 { bounds.size.height } else { size.height };
            let frame = CGRect {
                origin: CGPoint { x: (wheel.size.width - width) / 2.0,
                    y: wheel.size.height / 2.0 + (row as f32 - position) * size.height - height / 2.0 },
                size: CGSize { width, height },
            };
            () = msg![env; view setFrame:frame];
            if row_views_current(env, picker, revision) {
                () = msg![env; container addSubview:view];
                () = msg![env; view layoutIfNeeded];
            }
        }
        release(env, view);
    }
    for view in reusable { release(env, view); }
    if row_views_current(env, picker, revision) {
        let host = env.objc.borrow::<UIPickerViewHostObject>(picker);
        let shows = host.shows_selection_indicator;
        let mut indicator = host.row_indicators.get(&component).copied().unwrap_or(nil);
        if indicator == nil {
            indicator = msg_class![env; _touchHLEPickerSelection alloc];
            indicator = msg![env; indicator initWithFrame:(CGRect::default())];
            () = msg![env; indicator setOpaque:false];
            () = msg![env; indicator setUserInteractionEnabled:false];
            env.objc.borrow_mut::<UIPickerViewHostObject>(picker).row_indicators.insert(component, indicator);
        }
        let frame = CGRect {
            origin: CGPoint { x: 0.0, y: (wheel.size.height - size.height) / 2.0 },
            size: CGSize { width: wheel.size.width, height: size.height },
        };
        () = msg![env; indicator setFrame:frame];
        () = msg![env; indicator setHidden:(!shows)];
        () = msg![env; container addSubview:indicator];
        () = msg![env; indicator setNeedsDisplay];
    }
    release(env, container);
}

fn visible_rows(position: f32, height: f32, row_height: f32, count: NSUInteger) -> (NSUInteger, NSUInteger) {
    if count == 0 || !height.is_finite() || height <= 0.0
        || !row_height.is_finite() || row_height <= 0.0 || !position.is_finite() { return (0, 0); }
    let half = height / (2.0 * row_height);
    let first = (position - half - 0.5).ceil().max(0.0).min(count as f32) as NSUInteger;
    let end = ((position + half + 0.5).floor() + 1.0).max(0.0).min(count as f32) as NSUInteger;
    (first, end.max(first))
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIPickerView: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(UIPickerViewHostObject {
        superclass: UIViewHostObject::default(),
        delegate: nil,
        data_source: nil,
        shows_selection_indicator: false,
        number_of_components: 0,
        selected_rows: Default::default(),
        positions: Default::default(),
        animations: Default::default(),
        drag: None,
        timer: nil,
        row_views: HashMap::new(),
        row_containers: HashMap::new(),
        row_indicators: HashMap::new(),
        rows_revision: 0,
        updating_rows: false,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (())drawRect:(CGRect)_rect {
    () = msg![env; this _touchHLEUpdateRowViews];
    use ios5_theme::{draw_surface, inset_rect, rgb};
    use crate::frameworks::core_graphics::cg_context::{CGContextSaveGState, CGContextRestoreGState, CGContextClipToRect};
    let ctx = crate::frameworks::uikit::ui_graphics::UIGraphicsGetCurrentContext(env);
    if ctx == nil { return; }
    let bounds: CGRect = msg![env; this bounds];
    draw_surface(env, ctx, bounds, 0.0,
        &[(0.0, rgb(0x333435)), (1.0, rgb(0x737374))], rgb(0x111111));
    let host = env.objc.borrow::<UIPickerViewHostObject>(this);
    let (delegate, count, indicator, selections, positions) = (
        host.delegate, host.number_of_components, host.shows_selection_indicator,
        host.selected_rows.clone(), host.positions.clone(),
    );
    if count == 0 { return; }
    let font: id = msg_class![env; UIFont boldSystemFontOfSize:20.0f32];
    let color: id = msg_class![env; UIColor blackColor];
    let title_sel = env.objc.register_host_selector("pickerView:titleForRow:forComponent:".into(), &mut env.mem);
    let titles: bool = msg![env; delegate respondsToSelector:title_sel];
    let view_sel = env.objc.register_host_selector(
        "pickerView:viewForRow:forComponent:reusingView:".into(), &mut env.mem);
    let custom: bool = msg![env; delegate respondsToSelector:view_sel];
    let titles = titles && !custom;
    for component in 0..count {
        let frame: CGRect = msg![env; this _touchHLEFrameForComponent:component];
        let size: CGSize = msg![env; this rowSizeForComponent:component];
        let row_height = size.height;
        if !row_height.is_finite() || row_height <= 0.0 { continue; }
        let wheel = inset_rect(frame, 4.0, 8.0);
        draw_surface(env, ctx, wheel, 4.0, &[
            (0.0, rgb(0x9A9A9B)), (0.25, rgb(0xE4E4E7)),
            (0.5, rgb(0xFFFFFF)), (0.75, rgb(0xE4E4E7)), (1.0, rgb(0x9A9A9B)),
        ], rgb(0x333435));
        let center = wheel.origin.y + wheel.size.height / 2.0;
        if indicator && !custom {
            draw_surface(env, ctx, CGRect {
                origin: CGPoint { x: wheel.origin.x, y: center - row_height / 2.0 },
                size: CGSize { width: wheel.size.width, height: row_height },
            }, 0.0, &[(0.0, (0.63, 0.70, 0.79, 0.5)), (1.0, (0.46, 0.55, 0.67, 0.25))], rgb(0x798EAC));
        }
        if !titles { continue; }
        let rows: NSUInteger = msg![env; this numberOfRowsInComponent:component];
        if rows == 0 { continue; }
        let selected = selections.get(&component).copied().unwrap_or(0).min(rows - 1);
        let position = positions.get(&component).copied().unwrap_or(selected as f32);
        let half = wheel.size.height / (2.0 * row_height) + 1.0;
        let first = ((position - half).floor() as i64).max(0);
        let last = ((position + half).ceil() as i64).min(rows as i64 - 1);
        CGContextSaveGState(env, ctx);
        CGContextClipToRect(env, ctx, wheel);
        for row in first..=last {
            let title: id = msg![env; delegate pickerView:this titleForRow:(row as NSUInteger) forComponent:component];
            let size: CGSize = msg![env; title sizeWithFont:font];
            () = msg![env; color set];
            let point = CGPoint { x: wheel.origin.x + (wheel.size.width - size.width) / 2.0,
                y: center + (row as f32 - position) * row_height - size.height / 2.0 };
            let _: CGSize = msg![env; title drawAtPoint:point withFont:font];
        }
        CGContextRestoreGState(env, ctx);
    }
}

- (id)initWithFrame:(CGRect)frame {
    msg_super![env; this initWithFrame:frame]
}

- (())layoutSubviews {
    () = msg_super![env; this layoutSubviews];
    () = msg![env; this setNeedsDisplay];
}

- (())setNeedsDisplay {
    () = msg_super![env; this setNeedsDisplay];
    () = msg![env; this _touchHLEUpdateRowViews];
}

- (())_touchHLEUpdateRowViews {
    let host = env.objc.borrow_mut::<UIPickerViewHostObject>(this);
    if host.updating_rows { return; }
    host.updating_rows = true;
    let (delegate, revision) = (host.delegate, host.rows_revision);
    retain(env, this);
    retain(env, delegate);
    update_row_views(env, this, delegate, revision);
    release(env, delegate);
    env.objc.borrow_mut::<UIPickerViewHostObject>(this).updating_rows = false;
    release(env, this);
}

- (())dealloc {
    env.objc.borrow_mut::<UIPickerViewHostObject>(this).updating_rows = true;
    clear_row_views(env, this, None);
    let host = env.objc.borrow::<UIPickerViewHostObject>(this);
    let (delegate, data_source, touch) = (
        host.delegate, host.data_source, host.drag.as_ref().map_or(nil, |drag| drag.touch),
    );
    release(env, touch);
    release(env, delegate);
    release(env, data_source);
    msg_super![env; this dealloc]
}

// MARK: - Delegate

- (id)delegate {
    env.objc.borrow::<UIPickerViewHostObject>(this).delegate
}

- (())setDelegate:(id)delegate {
    () = msg![env; this _touchHLEStopWheelMotion];
    retain(env, delegate);
    let old = std::mem::replace(&mut env.objc.borrow_mut::<UIPickerViewHostObject>(this).delegate, delegate);
    clear_row_views(env, this, None);
    release(env, old);
    () = msg![env; this setNeedsDisplay];
}

// MARK: - Data source

- (id)dataSource {
    env.objc.borrow::<UIPickerViewHostObject>(this).data_source
}

- (())setDataSource:(id)data_source {
    () = msg![env; this _touchHLEStopWheelMotion];
    retain(env, data_source);
    let old = std::mem::replace(&mut env.objc.borrow_mut::<UIPickerViewHostObject>(this).data_source, data_source);
    env.objc.borrow_mut::<UIPickerViewHostObject>(this).number_of_components = 0;
    clear_row_views(env, this, None);
    release(env, old);
    // Refresh component count from the new data source.
    let count: NSUInteger = if data_source != nil {
        msg![env; data_source numberOfComponentsInPickerView:this]
    } else {
        0
    };
    env.objc.borrow_mut::<UIPickerViewHostObject>(this).number_of_components = count;
    () = msg![env; this setNeedsDisplay];
}

// MARK: - Selection indicator

- (bool)showsSelectionIndicator {
    env.objc.borrow::<UIPickerViewHostObject>(this).shows_selection_indicator
}

- (())setShowsSelectionIndicator:(bool)shows {
    env.objc.borrow_mut::<UIPickerViewHostObject>(this).shows_selection_indicator = shows;
    () = msg![env; this setNeedsDisplay];
}

// MARK: - Component / row counts

- (NSUInteger)numberOfComponents {
    env.objc.borrow::<UIPickerViewHostObject>(this).number_of_components
}

- (NSUInteger)numberOfRowsInComponent:(NSUInteger)component {
    let data_source = env.objc.borrow::<UIPickerViewHostObject>(this).data_source;
    if data_source == nil {
        return 0;
    }
    msg![env; data_source pickerView:this numberOfRowsInComponent:component]
}

// MARK: - Row size (delegate query)

- (CGSize)rowSizeForComponent:(NSUInteger)component {
    let host = env.objc.borrow::<UIPickerViewHostObject>(this);
    let (delegate, count) = (host.delegate, host.number_of_components);
    if component >= count { return CGSize::default(); }
    let bounds: CGRect = msg![env; this bounds];
    let mut size = CGSize { width: bounds.size.width / count as f32, height: 44.0 };
    for (name, is_width) in [("pickerView:widthForComponent:", true), ("pickerView:rowHeightForComponent:", false)] {
        let sel = env.objc.register_host_selector(name.into(), &mut env.mem);
        let responds: bool = msg![env; delegate respondsToSelector:sel];
        if responds {
            let value: f32 = if is_width {
                msg![env; delegate pickerView:this widthForComponent:component]
            } else { msg![env; delegate pickerView:this rowHeightForComponent:component] };
            if value.is_finite() && value > 0.0 {
                if is_width { size.width = value; } else { size.height = value; }
            }
        }
    }
    size
}

- (())_touchHLEStartWheelTimer {
    if env.objc.borrow::<UIPickerViewHostObject>(this).timer != nil { return; }
    let sel = env.objc.register_host_selector("_touchHLEWheelTick:".into(), &mut env.mem);
    let timer: id = msg_class![env; NSTimer scheduledTimerWithTimeInterval:(1.0f64 / 60.0)
        target:this selector:sel userInfo:nil repeats:true];
    retain(env, timer);
    env.objc.borrow_mut::<UIPickerViewHostObject>(this).timer = timer;
}

- (())_touchHLEStopWheelMotion {
    retain(env, this);
    let host = env.objc.borrow_mut::<UIPickerViewHostObject>(this);
    let timer = std::mem::replace(&mut host.timer, nil);
    let touch = host.drag.take().map_or(nil, |drag| drag.touch);
    host.animations.clear();
    host.positions.clear();
    () = msg![env; timer invalidate];
    release(env, timer);
    release(env, touch);
    () = msg_super![env; this setNeedsDisplay];
    release(env, this);
}

- (())didMoveToWindow {
    let window: id = msg![env; this window];
    if window == nil { () = msg![env; this _touchHLEStopWheelMotion]; }
}

- (())_touchHLEWheelTick:(id)timer {
    if env.objc.borrow::<UIPickerViewHostObject>(this).timer != timer { return; }
    retain(env, this);
    let now = Instant::now();
    let mut finished = Vec::new();
    {
        let host = env.objc.borrow_mut::<UIPickerViewHostObject>(this);
        for (&component, animation) in &host.animations {
            let t = now.duration_since(animation.started).as_secs_f32() / animation.duration;
            let position = animation_position(animation.from, animation.to, t);
            host.positions.insert(component, position);
            if t >= 1.0 { finished.push((component, animation.to as NSUInteger, animation.notify)); }
        }
        for &(component, row, _) in &finished {
            host.animations.remove(&component);
            host.selected_rows.insert(component, row);
        }
    }
    if env.objc.borrow::<UIPickerViewHostObject>(this).animations.is_empty() {
        env.objc.borrow_mut::<UIPickerViewHostObject>(this).timer = nil;
        () = msg![env; timer invalidate];
        release(env, timer);
    }
    () = msg![env; this setNeedsDisplay];
    for (component, row, notify) in finished {
        if !notify { continue; }
        let host = env.objc.borrow::<UIPickerViewHostObject>(this);
        if component >= host.number_of_components || host.selected_rows.get(&component) != Some(&row) { continue; }
        let delegate = host.delegate;
        let sel = env.objc.register_host_selector("pickerView:didSelectRow:inComponent:".into(), &mut env.mem);
        let responds: bool = msg![env; delegate respondsToSelector:sel];
        if responds { () = msg![env; delegate pickerView:this didSelectRow:row inComponent:component]; }
    }
    release(env, this);
}

- (CGRect)_touchHLEFrameForComponent:(NSUInteger)component {
    let count: NSUInteger = msg![env; this numberOfComponents];
    if component >= count { return CGRect::default(); }
    let bounds: CGRect = msg![env; this bounds];
    let mut widths = Vec::new();
    for i in 0..count {
        let size: CGSize = msg![env; this rowSizeForComponent:i];
        widths.push(size.width.max(1.0));
    }
    let total: f32 = widths.iter().sum();
    let factor = (bounds.size.width / total).min(1.0).max(0.0);
    let x = bounds.origin.x + (bounds.size.width - total * factor) / 2.0
        + widths[..component as usize].iter().sum::<f32>() * factor;
    CGRect { origin: CGPoint { x, y: bounds.origin.y },
        size: CGSize { width: widths[component as usize] * factor, height: bounds.size.height } }
}

- (())touchesBegan:(id)touches withEvent:(id)_event {
    if env.objc.borrow::<UIPickerViewHostObject>(this).drag.is_some() { return; }
    let touch: id = msg![env; touches anyObject];
    if touch == nil { return; }
    let point: CGPoint = msg![env; touch locationInView:this];
    let timestamp: f64 = msg![env; touch timestamp];
    if !point.y.is_finite() || !timestamp.is_finite() { return; }
    let count: NSUInteger = msg![env; this numberOfComponents];
    for component in 0..count {
        let frame: CGRect = msg![env; this _touchHLEFrameForComponent:component];
        if point.x < frame.origin.x || point.x >= frame.origin.x + frame.size.width
            || point.y < frame.origin.y || point.y >= frame.origin.y + frame.size.height { continue; }
        let rows: NSUInteger = msg![env; this numberOfRowsInComponent:component];
        if rows == 0 { return; }
        retain(env, touch);
        let host = env.objc.borrow_mut::<UIPickerViewHostObject>(this);
        let position = host.positions.get(&component).copied().unwrap_or(
            host.selected_rows.get(&component).copied().unwrap_or(0) as f32);
        host.animations.remove(&component);
        host.drag = Some(WheelDrag { touch, component, start_y: point.y, last_y: point.y,
            timestamp, raw_position: position, velocity: 0.0, moved: false });
        break;
    }
}

- (())touchesMoved:(id)touches withEvent:(id)_event {
    let Some(drag) = env.objc.borrow::<UIPickerViewHostObject>(this).drag.as_ref() else { return; };
    let (touch, component) = (drag.touch, drag.component);
    let contains: bool = msg![env; touches containsObject:touch];
    if !contains { return; }
    let point: CGPoint = msg![env; touch locationInView:this];
    let timestamp: f64 = msg![env; touch timestamp];
    let size: CGSize = msg![env; this rowSizeForComponent:component];
    let rows: NSUInteger = msg![env; this numberOfRowsInComponent:component];
    if rows == 0 || size.height <= 0.0 || !point.y.is_finite() || !timestamp.is_finite() { return; }
    let host = env.objc.borrow_mut::<UIPickerViewHostObject>(this);
    let Some(drag) = host.drag.as_mut() else { return; };
    let delta = (drag.last_y - point.y) / size.height;
    let dt = timestamp - drag.timestamp;
    if dt > 0.0 {
        drag.velocity = if dt > 0.15 { 0.0 } else {
            0.25 * drag.velocity + 0.75 * (delta / dt as f32).clamp(-50.0, 50.0)
        };
    }
    drag.raw_position = (drag.raw_position + delta).clamp(-3.0, rows as f32 + 2.0);
    drag.last_y = point.y;
    drag.timestamp = timestamp;
    drag.moved |= (point.y - drag.start_y).abs() > 4.0;
    let position = rubber_band(drag.raw_position, (rows - 1) as f32);
    host.positions.insert(component, position);
    () = msg![env; this setNeedsDisplay];
}

// MARK: - Selection

- (NSUInteger)selectedRowInComponent:(NSUInteger)component {
    if component >= env.objc.borrow::<UIPickerViewHostObject>(this).number_of_components { return 0; }
    let count: NSUInteger = msg![env; this numberOfRowsInComponent:component];
    env.objc.borrow::<UIPickerViewHostObject>(this).selected_rows.get(&component)
        .copied().unwrap_or(0).min(count.saturating_sub(1))
}

- (())selectRow:(NSUInteger)row
    inComponent:(NSUInteger)component
        animated:(bool)animated {
    if component >= env.objc.borrow::<UIPickerViewHostObject>(this).number_of_components { return; }
    let count: NSUInteger = msg![env; this numberOfRowsInComponent:component];
    if row >= count { return; }
    let host = env.objc.borrow_mut::<UIPickerViewHostObject>(this);
    let from = host.positions.get(&component).copied().unwrap_or(
        host.selected_rows.get(&component).copied().unwrap_or(0) as f32);
    let touch = if host.drag.as_ref().is_some_and(|d| d.component == component) {
        host.drag.take().unwrap().touch
    } else { nil };
    host.selected_rows.insert(component, row);
    host.animations.remove(&component);
    if animated {
        host.animations.insert(component, WheelAnimation {
            from, to: row as f32, started: Instant::now(), duration: 0.3, notify: false,
        });
    } else { host.positions.insert(component, row as f32); }
    release(env, touch);
    if animated { () = msg![env; this _touchHLEStartWheelTimer]; }
    () = msg![env; this setNeedsDisplay];
}

- (())touchesEnded:(id)touches withEvent:(id)event {
    let Some(drag) = env.objc.borrow::<UIPickerViewHostObject>(this).drag.as_ref() else { return; };
    let (touch, last_y) = (drag.touch, drag.last_y);
    let contains: bool = msg![env; touches containsObject:touch];
    if !contains { return; }
    let point: CGPoint = msg![env; touch locationInView:this];
    let timestamp: f64 = msg![env; touch timestamp];
    // Consume a final movement even when the platform omitted a move event.
    if !point.y.is_finite() || !timestamp.is_finite() {
        () = msg![env; this touchesCancelled:touches withEvent:event];
        return;
    }
    if point.y != last_y { () = msg![env; this touchesMoved:touches withEvent:event]; }
    let Some(drag) = env.objc.borrow_mut::<UIPickerViewHostObject>(this).drag.take() else { return; };
    let component = drag.component;
    let rows: NSUInteger = msg![env; this numberOfRowsInComponent:component];
    let size: CGSize = msg![env; this rowSizeForComponent:component];
    let frame: CGRect = msg![env; this _touchHLEFrameForComponent:component];
    release(env, drag.touch);
    if rows == 0 || size.height <= 0.0 { return; }
    let host = env.objc.borrow_mut::<UIPickerViewHostObject>(this);
    let from = host.positions.get(&component).copied().unwrap_or(drag.raw_position);
    let velocity = if timestamp - drag.timestamp > 0.12 { 0.0 } else { drag.velocity };
    let to = if drag.moved { release_target(from, velocity, (rows - 1) as f32) }
        else { (from + (point.y - frame.origin.y - frame.size.height / 2.0) / size.height)
            .round().clamp(0.0, (rows - 1) as f32) };
    let changed = host.selected_rows.get(&component).copied().unwrap_or(0) != to as NSUInteger;
    host.animations.insert(component, WheelAnimation {
        from, to, started: Instant::now(),
        duration: (0.18 + (to - from).abs() * 0.035).min(0.75), notify: changed,
    });
    () = msg![env; this _touchHLEStartWheelTimer];
}

- (())touchesCancelled:(id)touches withEvent:(id)_event {
    let Some(drag) = env.objc.borrow::<UIPickerViewHostObject>(this).drag.as_ref() else { return; };
    let touch = drag.touch;
    let contains: bool = msg![env; touches containsObject:touch];
    if !contains { return; }
    let host = env.objc.borrow_mut::<UIPickerViewHostObject>(this);
    let drag = host.drag.take().unwrap();
    let component = drag.component;
    let from = host.positions.get(&component).copied().unwrap_or(drag.raw_position);
    let to = host.selected_rows.get(&component).copied().unwrap_or(0) as f32;
    host.animations.insert(component, WheelAnimation {
        from, to, started: Instant::now(), duration: 0.2, notify: false,
    });
    release(env, drag.touch);
    () = msg![env; this _touchHLEStartWheelTimer];
}

// MARK: - View for row (delegate query)

- (id)viewForRow:(NSUInteger)row
    forComponent:(NSUInteger)component {
    let delegate = env.objc.borrow::<UIPickerViewHostObject>(this).delegate;
    if delegate == nil {
        return nil;
    }
    env.objc.borrow::<UIPickerViewHostObject>(this).row_views
        .get(&(component, row)).copied().unwrap_or(nil)
}

// MARK: - Reload

- (())reloadAllComponents {
    () = msg![env; this _touchHLEStopWheelMotion];
    log_dbg!("UIPickerView reloadAllComponents");
    clear_row_views(env, this, None);
    // Refresh component count.
    let data_source = env.objc.borrow::<UIPickerViewHostObject>(this).data_source;
    let count: NSUInteger = if data_source != nil {
        msg![env; data_source numberOfComponentsInPickerView:this]
    } else {
        0
    };
    env.objc.borrow_mut::<UIPickerViewHostObject>(this).number_of_components = count;
    () = msg![env; this setNeedsDisplay];
}

- (())reloadComponent:(NSUInteger)component {
    if component >= env.objc.borrow::<UIPickerViewHostObject>(this).number_of_components { return; }
    () = msg![env; this _touchHLEStopWheelMotion];
    let count: NSUInteger = msg![env; this numberOfRowsInComponent:component];
    let host = env.objc.borrow_mut::<UIPickerViewHostObject>(this);
    let row = host.selected_rows.entry(component).or_insert(0);
    *row = (*row).min(count.saturating_sub(1));
    clear_row_views(env, this, Some(component));
    () = msg![env; this setNeedsDisplay];
}

@end

@implementation _touchHLEPickerSelection: UIView
- (())drawRect:(CGRect)_rect {
    let ctx = crate::frameworks::uikit::ui_graphics::UIGraphicsGetCurrentContext(env);
    if ctx == nil { return; }
    let bounds: CGRect = msg![env; this bounds];
    ios5_theme::draw_surface(env, ctx, bounds, 0.0,
        &[(0.0, (0.63, 0.70, 0.79, 0.5)), (1.0, (0.46, 0.55, 0.67, 0.25))],
        ios5_theme::rgb(0x798EAC));
}
@end

};

#[cfg(test)]
mod tests {
    use super::{animation_position, release_target, rubber_band, visible_rows};

    #[test]
    fn custom_rows_follow_fractional_position() {
        assert_eq!(visible_rows(5.0, 132.0, 44.0, 20), (3, 8));
        assert_eq!(visible_rows(5.25, 132.0, 44.0, 20), (4, 8));
        assert_eq!(visible_rows(0.0, 132.0, 44.0, 20), (0, 3));
        assert_eq!(visible_rows(19.0, 132.0, 44.0, 20), (17, 20));
        assert_eq!(visible_rows(0.0, 132.0, 44.0, 0), (0, 0));
        assert_eq!(visible_rows(0.0, 0.0, 44.0, 20), (0, 0));
    }

    #[test]
    fn wheel_targets_stay_in_bounds() {
        assert_eq!(release_target(0.0, -50.0, 9.0), 0.0);
        assert_eq!(release_target(9.0, 50.0, 9.0), 9.0);
        assert_eq!(release_target(3.6, 0.0, 9.0), 4.0);
        assert_eq!(release_target(0.0, 50.0, 0.0), 0.0);
    }

    #[test]
    fn wheel_animation_settles_monotonically() {
        assert_eq!(animation_position(2.0, 8.0, 0.0), 2.0);
        assert_eq!(animation_position(2.0, 8.0, 1.0), 8.0);
        let mut previous = 2.0;
        for i in 1..=100 {
            let next = animation_position(2.0, 8.0, i as f32 / 100.0);
            assert!(next >= previous && next <= 8.0);
            previous = next;
        }
    }

    #[test]
    fn wheel_edges_resist_overdrag() {
        assert_eq!(rubber_band(4.0, 9.0), 4.0);
        assert!(rubber_band(-10.0, 9.0) > -0.5);
        assert!(rubber_band(19.0, 9.0) < 9.5);
    }
}