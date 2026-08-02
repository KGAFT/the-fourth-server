//! Minimal structure-type + message used by the tf echo servers/clients.

use num_enum::TryFromPrimitive;
use std::any::{Any, TypeId};
use std::hash::{DefaultHasher, Hash, Hasher};
use rkyv::{Archive, Deserialize, Serialize};
use tfserver::{impl_strong_type, impl_structure_type};
use tfserver::structures::s_type::{StrongType, StructureType};

#[repr(u8)]
#[derive(Serialize, Deserialize, PartialEq, Clone, Hash, Eq, TryFromPrimitive, Copy, Debug, Archive)]
pub enum EchoSType {
    Echo,
}

impl_structure_type!(
    EchoSType, ArchivedEchoSType,
    Echo => (EchoMsg, ArchivedEchoMsg),
);

impl_strong_type!(
    EchoMsg => ArchivedEchoMsg,
);

#[derive(Serialize, Deserialize, Debug, Archive)]
pub struct EchoMsg {
    pub s_type: EchoSType,
    pub data: Vec<u8>,
}

