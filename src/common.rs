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

use std::collections::HashMap;
use std::error::Error;

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync + 'static>>;

/// Trait for clipboard access
pub trait ClipboardProvider: Send {
    /// Method to get the clipboard contents as a String
    fn get_contents(&self) -> Result<String>;
    /// Method to set the clipboard contents as a String
    fn set_contents(&self, _: String) -> Result<()>;
    /// Get the list of content types supported by the current clipboard item. Content types
    /// are returned normalized.
    fn get_content_types(&self) -> Result<Vec<ContentType>> {
        Err("unsupported for this platform".into())
    }
    /// Get data for a particular content type
    fn get_content_for_type(&self, _ct: &ContentType) -> Result<Vec<u8>> {
        Err("unsupported for this platform".into())
    }
    /// Set the mapping of content types to data in the clipboard
    fn set_content_types(&self, _map: HashMap<ContentType, Vec<u8>>) -> Result<()> {
        Err("unsupported for this platform".into())
    }
    /// Normalize a content type, ensuring it is not a [`ContentType::Custom`] instance if it
    /// can be represented as another member of [`ContentType`].
    fn normalize_content_type(_ct: ContentType) -> ContentType {
        todo!("not implemented for this platform")
    }
    /// Denormalize content type. The resulting string can be used to create a
    /// [`ContentType::Custom`] instance.
    fn denormalize_content_type(_ct: ContentType) -> String {
        todo!("not implemented for this platform")
    }
}

/// Represents the type or format of cotnent in the clipboard. On most systems, a single clipboard
/// item may contain alternate representations in different formats.
///
/// This enum defines portable values for common formats, as well as a [`ContentType::Custom`]
/// alternative to represent system-specific types.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum ContentType {
    Text,
    Html,
    Pdf,
    Png,
    Rtf,
    Url,
    Custom(String),
}
