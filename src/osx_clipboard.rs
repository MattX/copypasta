// Copyright 2016 Avraham Weinstock
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::mem::transmute;
use std::sync::{Mutex, MutexGuard};

use lazy_static::lazy_static;
use objc::runtime::{Class, Object};
use objc::{msg_send, sel, sel_impl};
use objc_foundation::{INSArray, INSObject, INSString, INSFastEnumeration};
use objc_foundation::{NSArray, NSDictionary, NSObject, NSString};
use objc_id::{Id, Owned};

use crate::common::*;

struct ClipboardMutexToken;

// creating or accessing the context is not thread-safe, and needs to be protected
lazy_static! {
    static ref CLIPBOARD_CONTEXT_MUTEX: Mutex<ClipboardMutexToken> = Mutex::new(ClipboardMutexToken {});
}

pub struct OSXClipboardContext {
    pasteboard: Id<Object>,
}

// required to bring NSPasteboard into the path of the class-resolver
#[link(name = "AppKit", kind = "framework")]
extern "C" {}

impl OSXClipboardContext {
    pub fn new() -> Result<OSXClipboardContext> {
        let lock = CLIPBOARD_CONTEXT_MUTEX.lock();
        if !lock.is_ok() {
            return Err("could not acquire mutex".into());
        }
        let cls = Class::get("NSPasteboard").ok_or("Class::get(\"NSPasteboard\")")?;
        let pasteboard: *mut Object = unsafe { msg_send![cls, generalPasteboard] };
        if pasteboard.is_null() {
            return Err("NSPasteboard#generalPasteboard returned null".into());
        }
        let pasteboard: Id<Object> = unsafe { Id::from_ptr(pasteboard) };
        Ok(OSXClipboardContext { pasteboard })
    }

    fn first_item(&self, _guard: &mut MutexGuard<ClipboardMutexToken>) -> Option<&NSObject> {
        unsafe {
            // TODO I don't understand the memory model here. The NSArray we get is a copy, as
            //      seen in https://developer.apple.com/documentation/appkit/nspasteboard/1529995-pasteboarditems?language=objc
            //      but the elements themselves are not. So when are they deallocated? On the next call
            //      to pasteboardItems? That seems wasteful?
            let items: *mut NSArray<NSObject> = msg_send![self.pasteboard, pasteboardItems];
            if items.is_null() {
                return None;
            }
            let _id_items: Id<NSArray<_>, Owned> = Id::from_ptr(items);
            (&*items).first_object()
        }
    }
}

impl ClipboardProvider for OSXClipboardContext {
    fn get_contents(&self) -> Result<String> {
        let lock = CLIPBOARD_CONTEXT_MUTEX.lock();
        if !lock.is_ok() {
            return Err("could not acquire mutex".into());
        }
        let string_class: Id<NSObject> = {
            let cls: Id<Class> = unsafe { Id::from_ptr(class("NSString")) };
            unsafe { transmute(cls) }
        };
        let classes: Id<NSArray<NSObject, Owned>> = NSArray::from_vec(vec![string_class]);
        let options: Id<NSDictionary<NSObject, NSObject>> = NSDictionary::new();
        let string_array: Id<NSArray<NSString>> = unsafe {
            let obj: *mut NSArray<NSString> =
                msg_send![self.pasteboard, readObjectsForClasses:&*classes options:&*options];
            if obj.is_null() {
                return Err("pasteboard#readObjectsForClasses:options: returned null".into());
            }
            Id::from_ptr(obj)
        };
        if string_array.count() == 0 {
            Err("pasteboard#readObjectsForClasses:options: returned empty".into())
        } else {
            Ok(string_array[0].as_str().to_owned())
        }
    }

    fn set_contents(&self, data: String) -> Result<()> {
        let lock = CLIPBOARD_CONTEXT_MUTEX.lock();
        if !lock.is_ok() {
            return Err("could not acquire mutex".into());
        }
        let string_array = NSArray::from_vec(vec![NSString::from_str(&data)]);
        let _: usize = unsafe { msg_send![self.pasteboard, clearContents] };
        let success: bool = unsafe { msg_send![self.pasteboard, writeObjects: string_array] };
        if success {
            Ok(())
        } else {
            Err("NSPasteboard#writeObjects: returned false".into())
        }
    }

    fn get_content_types(&self) -> Result<Vec<ContentType>> {
        let lock = CLIPBOARD_CONTEXT_MUTEX.lock();
        if !lock.is_ok() {
            return Err("could not acquire mutex".into());
        }
        let first_item = self.first_item(&mut lock.unwrap());
        if first_item.is_none() {
            return Ok(Vec::new());
        }
        let types: Id<NSArray<NSString>, Owned> = unsafe {
            let types: *mut NSArray<NSString> = msg_send![first_item.unwrap(), types];
            Id::from_ptr(types)
        };
        Ok(types.enumerator().into_iter().map(|t| ContentType::Custom(t.as_str().into())).collect())
    }
}

// This is stolen from
// https://github.com/ryanmcgrath/cacao/blob/87eda8f94af8c15eef5e2e3386764407f21d53eb/src/pasteboard/types.rs#L83-L103
//
// TODO: I think it should be possible to access values from the appropriate struct instead of
//       hardcoding them here?
//       https://developer.apple.com/documentation/appkit/nspasteboard/pasteboardtype
//       I'm not really sure how this works though. Do I need some sort of bindgen?
impl From<ContentType> for Result<Id<NSString, Owned>> {
    fn from(pboard_type: ContentType) -> Self {
        Ok(NSString::from_str(&match pboard_type {
            ContentType::Url => "public.file-url".to_owned(),
            ContentType::Html => "public.html".to_owned(),
            ContentType::Pdf => "com.adobe.pdf".to_owned(),
            ContentType::Png => "public.png".to_owned(),
            ContentType::Rtf => "public.rtf".to_owned(),
            ContentType::Text => "public.utf8-plain-text".to_owned(),
            ContentType::Custom(s) => s,
        }))
    }
}

// this is a convenience function that both cocoa-rs and
//  glutin define, which seems to depend on the fact that
//  Option::None has the same representation as a null pointer
#[inline]
pub fn class(name: &str) -> *mut Class {
    unsafe { transmute(Class::get(name)) }
}
