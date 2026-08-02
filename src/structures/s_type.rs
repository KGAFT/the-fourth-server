use num_enum::TryFromPrimitive;
use std::any::{Any, TypeId};
use std::collections::HashSet;
use std::fmt;
use std::hash::{DefaultHasher, Hash, Hasher};
use rkyv::{api::high::{HighSerializer, HighValidator}, rancor::{Error as RkyvError, Strategy}, ser::allocator::ArenaHandle, util::AlignedVec, Archive, Deserialize, Serialize};


#[derive(Debug, Serialize, Deserialize, Archive)]
pub enum ServerErrorEn {
    MalformedMetaInfo(Option<String>),
    NoSuchHandler(Option<String>),
    InternalError(Option<Vec<u8>>),
    PayloadLost,
}
#[derive(Serialize, Deserialize, Archive)]
pub struct ServerError {
    s_type: SystemSType,
    pub en: ServerErrorEn,
}

impl ServerError {
    pub fn new(en: ServerErrorEn) -> Self {
        Self {
            s_type: SystemSType::ServerError,
            en,
        }
    }
}

///The trait for you structure type enum. Needed for proper routing of packets and type safety on serialization/deserialization of data.
///
///You need to create your own structure type for every project.
///
///There are a bunch of functions that already exists, from other traits. They kept manualy due to dyn compatibility.
pub trait StructureType: Any + Send + Sync {
    fn get_type_id(&self) -> TypeId;
    ///We don't use the equals from rust trait, due to need of dyn compatibility
    fn equals(&self, other: &dyn StructureType) -> bool;

    fn as_any(&self) -> &dyn Any;
    ///Only for local use, do not try to use this in serialize functions
    fn hash(&self) -> u64;

    ///We don't use the equals from rust trait, due to need of dyn compatibility
    fn clone_unique(&self) -> Box<dyn StructureType>;

    ///Returns the pointer to function that deserializes value into the current structure type
    fn get_deserialize_function(&self) -> Box<dyn Fn(u64) -> Box<dyn StructureType>>;
    ///Returns the pointer to function that serializes structure type value into the u64 value.
    fn get_serialize_function(&self) -> Box<dyn Fn(Box<dyn StructureType>) -> u64>;
}

///Needed to be applied to the serializable/deserializable structures
pub trait StrongType: Any {
    ///Need to return reference of structure type enum inside structure
    fn get_s_type(&self) -> &dyn StructureType;
}


/// Implements `StructureType` for a fieldless "s_type" enum and its rkyv-archived
/// counterpart, given the list of (variant => (owned_struct, archived_struct)) mappings.
///
/// Requires: the owned enum derives Copy, Clone, PartialEq, Eq, TryFromPrimitive, repr(u8).
#[macro_export]
macro_rules! impl_structure_type {
    (
        $owned:ident, $archived:ident,
        $( $variant:ident => ($owned_struct:ty, $archived_struct:ty) ),+ $(,)?
    ) => {
        impl $owned {
            pub fn deserialize(val: u64) -> Box<dyn StructureType> {
                Box::new($owned::try_from(val as u8).unwrap())
            }

            pub fn serialize(refer: Box<dyn StructureType>) -> u64 {
                refer
                    .as_any()
                    .downcast_ref::<$owned>()
                    .unwrap()
                    .clone() as u8 as u64
            }
        }

        impl StructureType for $owned {
            fn get_type_id(&self) -> TypeId {
                match self {
                    $( $owned::$variant => TypeId::of::<$owned_struct>(), )+
                }
            }

            fn equals(&self, other: &dyn StructureType) -> bool {
                match other.as_any().downcast_ref::<Self>() {
                    Some(d) => d == self,
                    None => false,
                }
            }

            fn as_any(&self) -> &dyn Any {
                self
            }

            fn hash(&self) -> u64 {
                let mut hasher = DefaultHasher::default();
                TypeId::of::<Self>().hash(&mut hasher);
                (*self as u8).hash(&mut hasher);
                hasher.finish()
            }

            fn clone_unique(&self) -> Box<dyn StructureType> {
                Box::new(self.clone())
            }

            fn get_deserialize_function(&self) -> Box<dyn Fn(u64) -> Box<dyn StructureType>> {
                Box::new($owned::deserialize)
            }

            fn get_serialize_function(&self) -> Box<dyn Fn(Box<dyn StructureType>) -> u64> {
                Box::new($owned::serialize)
            }
        }

        impl $archived {
            fn to_owned_stype(&self) -> $owned {
                match self {
                    $( $archived::$variant => $owned::$variant, )+
                }
            }
        }

        impl StructureType for $archived {
            fn get_type_id(&self) -> TypeId {
                match self {
                    $( $archived::$variant => TypeId::of::<$archived_struct>(), )+
                }
            }

            fn equals(&self, other: &dyn StructureType) -> bool {
                match other.as_any().downcast_ref::<Self>() {
                    Some(d) => d.to_owned_stype() == self.to_owned_stype(),
                    None => false,
                }
            }

            fn as_any(&self) -> &dyn Any {
                self
            }

            fn hash(&self) -> u64 {
                let mut hasher = DefaultHasher::default();
                TypeId::of::<Self>().hash(&mut hasher);
                (self.to_owned_stype() as u8).hash(&mut hasher);
                hasher.finish()
            }

            fn clone_unique(&self) -> Box<dyn StructureType> {
                Box::new(self.to_owned_stype())
            }

            fn get_deserialize_function(&self) -> Box<dyn Fn(u64) -> Box<dyn StructureType>> {
                Box::new($owned::deserialize)
            }

            fn get_serialize_function(&self) -> Box<dyn Fn(Box<dyn StructureType>) -> u64> {
                Box::new($owned::serialize)
            }
        }
    };
}

/// Implements `StrongType` (owned + archived) for structs that carry an `s_type` field.
#[macro_export]
macro_rules! impl_strong_type {
    ($($owned:ty => $archived:ty),+ $(,)?) => {
        $(
            impl StrongType for $owned {
                fn get_s_type(&self) -> &dyn StructureType {
                    &self.s_type
                }
            }

            impl StrongType for $archived {
                fn get_s_type(&self) -> &dyn StructureType {
                    &self.s_type
                }
            }
        )+
    };
}


#[repr(u8)]
#[derive(Serialize, Deserialize, Archive, PartialEq, Clone, Hash, Eq, TryFromPrimitive, Copy)]
pub enum SystemSType {
    PacketMeta,
    HandlerMetaReq,
    HandlerMetaAns,
    ServerError,
}


impl fmt::Display for ServerErrorEn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerErrorEn::MalformedMetaInfo(Some(msg)) => {
                write!(f, "Malformed meta info: {}", msg)
            }
            ServerErrorEn::MalformedMetaInfo(None) => write!(f, "Malformed meta info!"),
            ServerErrorEn::NoSuchHandler(Some(msg)) => write!(f, "No such handler: {}", msg),
            ServerErrorEn::NoSuchHandler(None) => write!(f, "No such handler!"),
            ServerErrorEn::InternalError(Some(data)) => {
                write!(
                    f,
                    "{}",
                    String::from_utf8(data.clone())
                        .unwrap_or_else(|_| "Internal server error!".to_owned())
                )
            }
            ServerErrorEn::InternalError(None) => write!(f, "Internal server error!"),
            ServerErrorEn::PayloadLost => write!(f, "Payload lost!"),
        }
    }
}

impl std::error::Error for ServerErrorEn {}


 impl_structure_type!(
    SystemSType, ArchivedSystemSType,
    PacketMeta => (PacketMeta, ArchivedPacketMeta),
    HandlerMetaReq => (HandlerMetaReq, ArchivedHandlerMetaReq),
    HandlerMetaAns => (HandlerMetaAns, ArchivedHandlerMetaAns),
    ServerError => (ServerError, ArchivedServerError),
);

impl_strong_type!(
    PacketMeta => ArchivedPacketMeta,
    HandlerMetaReq => ArchivedHandlerMetaReq,
    HandlerMetaAns => ArchivedHandlerMetaAns,
    ServerError => ArchivedServerError,
);

#[derive(Serialize, Deserialize, Clone, Archive)]
pub struct PacketMeta {
    pub s_type: SystemSType,
    pub s_type_req: u64,
    pub handler_id: u64,
    pub has_payload: bool,
}



#[derive(Serialize, Deserialize, Clone, Archive)]
pub struct HandlerMetaReq {
    pub s_type: SystemSType,
    pub handler_name: String,
}

#[derive(Serialize, Deserialize, Clone, Archive)]
pub struct HandlerMetaAns {
    pub s_type: SystemSType,
    pub id: u64,
}





pub struct TypeContainer {
    s_type: Box<dyn StructureType>,
}

impl TypeContainer {
    pub fn new(s_type: Box<dyn StructureType>) -> Self {
        Self { s_type }
    }
}

impl PartialEq<Self> for TypeContainer {
    fn eq(&self, other: &Self) -> bool {
        self.s_type.equals(other.s_type.as_ref())
    }
}

impl Eq for TypeContainer {}

impl Hash for TypeContainer {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.s_type.hash().hash(state);
    }
}

impl Clone for TypeContainer {
    fn clone(&self) -> Self {
        Self {
            s_type: self.s_type.clone_unique(),
        }
    }
}

#[derive(Eq, Clone)]
pub struct TypeTuple {
    pub s_types: HashSet<TypeContainer>,
    pub handler_id: u64,
}

pub fn validate_s_type(target: &dyn StrongType) -> bool {
    let s_type = target.get_s_type();
    return s_type.get_type_id() == target.type_id();
}


type SerCtx<'a> = HighSerializer<AlignedVec, ArenaHandle<'a>, RkyvError>;
type ValCtx<'a> = HighValidator<'a, RkyvError>;


///Function that serializes object into binary data with type safety/
#[deprecated(note = "Use to_archive instead")]
pub fn to_vec<T>(arg: &T) -> Option<Vec<u8>>
where
    T: for<'a> Serialize<SerCtx<'a>> + StrongType,
{
    if !validate_s_type(arg) {
        eprintln!("stype validation failed");
        return None;
    }

    match rkyv::to_bytes::<RkyvError>(arg) {
        Ok(bytes) => Some(bytes.to_vec()),
        Err(_) => {
            eprintln!("rkyv serialization failed");
            None
        }
    }
}
/// Deserialize into an owned value.
#[deprecated(note = "Use from_archive instead")]
pub fn from_slice<T>(arg: &[u8]) -> Result<T, String>
where
    T: Archive + StrongType,
    T::Archived: for<'a> rkyv::bytecheck::CheckBytes<ValCtx<'a>>
    + Deserialize<T, Strategy<rkyv::de::Pool, RkyvError>>,
{
    let res: Result<T, RkyvError> = rkyv::from_bytes::<T, RkyvError>(arg);

    let parsed = match res {
        Ok(v) => v,
        Err(err) => {
            eprintln!("{}", err);
            return Err(decode_server_error(arg))
        },
    };

    if !validate_s_type(&parsed) {
        return Err(decode_server_error(arg));
    }

    Ok(parsed)
}

pub fn to_bytes<T>(arg: &T) -> Option<AlignedVec>
where
    T: for<'a> Serialize<SerCtx<'a>> + StrongType,
{
    if !validate_s_type(arg) {
        eprintln!("stype validation failed");
        return None;
    }

    rkyv::to_bytes::<RkyvError>(arg)
        .map_err(|_| eprintln!("rkyv serialization failed"))
        .ok()
}

/// Zero-copy deserialize: validates the bytes and hands back a reference
/// into `arg` (`&T::Archived`) instead of an owned `T`. No allocation,
/// no copy — this borrows from the input buffer.
pub fn access<'a, T>(arg: &'a [u8]) -> Result<&'a T::Archived, String>
where
    T: Archive + 'a,
    T::Archived: for<'b> rkyv::bytecheck::CheckBytes<ValCtx<'b>> + StrongType,
{
    let archived: &T::Archived = match rkyv::access::<T::Archived, RkyvError>(arg) {
        Ok(a) => a,
        Err(_) => return Err(decode_server_error(arg)),
    };

    if !validate_s_type(archived) {
        return Err(decode_server_error(arg));
    }

    Ok(archived)
}


fn decode_server_error(arg: &[u8]) -> String {
    match rkyv::from_bytes::<ServerError, RkyvError>(arg) {
        Ok(err) => err.en.to_string(),
        Err(_) => "Unknown packet type".to_string(),
    }
}

impl PartialEq<Self> for TypeTuple {
    fn eq(&self, other: &Self) -> bool {
        let iterator_list = if self.s_types.len() < other.s_types.len() {
            self.s_types.iter()
        } else {
            other.s_types.iter()
        };

        for s_type in iterator_list {
            if !self.s_types.contains(&s_type) {
                return false;
            }
        }
        self.handler_id == other.handler_id
    }
}

impl Hash for TypeTuple {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.handler_id.hash(state);
    }
}

