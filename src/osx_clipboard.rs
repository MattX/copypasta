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
use objc::runtime::{Class, Object, BOOL, NO};
use objc::{msg_send, sel, sel_impl};
use objc_foundation::{INSArray, INSData, INSFastEnumeration, INSObject, INSString, NSData};
use objc_foundation::{NSArray, NSDictionary, NSObject, NSString};
use objc_id::{Id, Owned};

use crate::common::*;
use std::collections::HashMap;

struct ClipboardMutexToken;

// creating or accessing the context is not thread-safe, and needs to be protected
lazy_static! {
    static ref CLIPBOARD_CONTEXT_MUTEX: Mutex<ClipboardMutexToken> =
        Mutex::new(ClipboardMutexToken {});
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
            panic!("could not acquire mutex");
        }
        let cls = Class::get("NSPasteboard").ok_or("Class::get(\"NSPasteboard\")")?;
        let pasteboard: *mut Object = unsafe { msg_send![cls, generalPasteboard] };
        if pasteboard.is_null() {
            return Err("NSPasteboard#generalPasteboard returned null".into());
        }
        let pasteboard: Id<Object> = unsafe { Id::from_ptr(pasteboard) };
        Ok(OSXClipboardContext { pasteboard })
    }

    /// Gets the first item from the pasteboard.
    ///
    /// Requires a mutex guard to prove that we hold the mutex. This method is called from methods
    /// which might need to hold the mutex, so it doesn't lock it itself.
    fn first_item(&self, _guard: &mut MutexGuard<ClipboardMutexToken>) -> Option<&NSObject> {
        unsafe {
            // TODO I don't understand the memory model here. The NSArray we get is a copy, as
            //      seen in https://developer.apple.com/documentation/appkit/nspasteboard/1529995-pasteboarditems?language=objc
            //      but the elements themselves are not. So when are they deallocated? On the next
            //      call to pasteboardItems? That seems wasteful?
            // TODO valgrind this
            let items: *mut NSArray<NSObject> = msg_send![self.pasteboard, pasteboardItems];
            if items.is_null() {
                return None;
            }
            let _id_items: Id<NSArray<NSObject>> = Id::from_ptr(items);
            (&*items).first_object()
        }
    }
}

impl ClipboardProvider for OSXClipboardContext {
    fn get_contents(&self) -> Result<String> {
        let lock = CLIPBOARD_CONTEXT_MUTEX.lock();
        if !lock.is_ok() {
            panic!("could not acquire mutex");
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
            panic!("could not acquire mutex");
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
            panic!("could not acquire mutex");
        }
        let first_item = self.first_item(&mut lock.unwrap());
        if first_item.is_none() {
            return Ok(Vec::new());
        }
        let types: Id<NSArray<NSString>> = unsafe {
            let types: *mut NSArray<NSString> = msg_send![first_item.unwrap(), types];
            Id::from_ptr(types)
        };
        Ok(types.enumerator().into_iter().map(|t| t.as_str().into()).collect())
    }

    fn get_content_for_type(&self, ct: &ContentType) -> Result<Vec<u8>> {
        let lock = CLIPBOARD_CONTEXT_MUTEX.lock();
        if !lock.is_ok() {
            panic!("could not acquire mutex");
        }
        let first_item = self.first_item(&mut lock.unwrap());
        if first_item.is_none() {
            return Err("clipboard is empty".into());
        }
        let typ: Id<NSString> = ct.into();
        let data: Id<NSData> = unsafe {
            let data: *mut NSData = msg_send![self.pasteboard, dataForType: typ];
            if data.is_null() {
                return Err(format!("clipboard does not have data for {:?}", &ct).into());
            }
            Id::from_ptr(data)
        };
        Ok(data.bytes().to_vec())
    }

    fn set_content_types(&self, map: HashMap<ContentType, Vec<u8>>) -> Result<()> {
        let lock = CLIPBOARD_CONTEXT_MUTEX.lock();
        if !lock.is_ok() {
            panic!("could not acquire mutex");
        }
        let cls = Class::get("NSPasteboardItem").ok_or("Class::get(\"NSPasteboardItem\")")?;
        let pasteboard_item: Id<NSObject> = unsafe {
            let item: *mut NSObject = msg_send![cls, new];
            Id::from_ptr(item)
        };
        for (ct, data) in map.into_iter() {
            let data = NSData::from_vec(data);
            let typ: Id<NSString> = (&ct).into();
            unsafe { msg_send![pasteboard_item, setData:data forType:typ] }
        }
        let items = NSArray::from_vec(vec![pasteboard_item]);
        let result: BOOL = unsafe {
            let _: () = msg_send![self.pasteboard, clearContents];
            msg_send![self.pasteboard, writeObjects: items]
        };
        if result == NO {
            Err("writeObject failed".into())
        } else {
            Ok(())
        }
    }

    fn normalize_content_type(ct: ContentType) -> ContentType {
        match &ct {
            ContentType::Custom(s) => s.into(),
            _ => ct,
        }
    }

    fn denormalize_content_type(ct: ContentType) -> String {
        match ct {
            ContentType::Url => "public.file-url",
            ContentType::Html => "public.html",
            ContentType::Pdf => "com.adobe.pdf",
            ContentType::Png => "public.png",
            ContentType::Rtf => "public.rtf",
            ContentType::Text => "public.utf8-plain-text",
            ContentType::Custom(s) => return s,
        }
        .into()
    }
}

// This is stolen from
// https://github.com/ryanmcgrath/cacao/blob/87eda8f94af8c15eef5e2e3386764407f21d53eb/src/pasteboard/types.rs#L83-L103
//
// TODO: I think it should be possible to access values from the appropriate struct instead of
//       hardcoding them here?
//       https://developer.apple.com/documentation/appkit/nspasteboard/pasteboardtype
//       I'm not really sure how this works though. Do I need some sort of bindgen?
impl<'a> From<&'a ContentType> for Id<NSString> {
    fn from(pboard_type: &'a ContentType) -> Self {
        NSString::from_str(&OSXClipboardContext::denormalize_content_type(pboard_type.clone()))
    }
}

impl<T: AsRef<str>> From<T> for ContentType {
    fn from(t: T) -> Self {
        match t.as_ref() {
            "public.file-url" => ContentType::Url,
            "public.html" => ContentType::Html,
            "com.adobe.pdf" => ContentType::Pdf,
            "public.png" => ContentType::Png,
            "public.rtf" => ContentType::Rtf,
            "public.utf8-plain-text" => ContentType::Text,
            other => ContentType::Custom(other.to_owned()),
        }
    }
}

// this is a convenience function that both cocoa-rs and
//  glutin define, which seems to depend on the fact that
//  Option::None has the same representation as a null pointer
#[inline]
pub fn class(name: &str) -> *mut Class {
    unsafe { transmute(Class::get(name)) }
}
