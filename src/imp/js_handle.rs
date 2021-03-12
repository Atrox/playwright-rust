use crate::imp::{core::*, prelude::*};

#[derive(Debug)]
pub(crate) struct JsHandle {
    channel: ChannelOwner,
    var: Mutex<Var>
}

#[derive(Debug)]
struct Var {
    preview: String
}

impl JsHandle {
    pub(crate) fn try_new(ctx: &Context, channel: ChannelOwner) -> Result<Self, Error> {
        let Initializer { preview } = serde_json::from_value(channel.initializer.clone())?;
        let var = Mutex::new(Var { preview });
        Ok(Self { channel, var })
    }

    pub(crate) async fn get_property(&self, name: &str) -> ArcResult<Weak<JsHandle>> {
        let mut args = HashMap::new();
        args.insert("name", name);
        let v = send_message!(self, "getProperty", args);
        let guid = only_guid(&v)?;
        let j = get_object!(self.context()?.lock().unwrap(), &guid, JsHandle)?;
        Ok(j)
    }

    pub(crate) async fn get_properties(&self) -> ArcResult<HashMap<String, Weak<JsHandle>>> {
        let v = send_message!(self, "getPropertyList", Map::new());
        let first = first(&v).ok_or(Error::InvalidParams)?;
        let properties: Vec<Property> =
            serde_json::from_value((*first).clone()).map_err(Error::Serde)?;
        let ps = properties
            .into_iter()
            .map(
                |Property {
                     name,
                     value: OnlyGuid { guid }
                 }| {
                    get_object!(self.context()?.lock().unwrap(), &guid, JsHandle).map(|o| (name, o))
                }
            )
            .collect::<Result<HashMap<_, _>, Error>>()?;
        Ok(ps)
    }

    pub(crate) async fn dispose(&self) -> ArcResult<()> {
        let _ = send_message!(self, "dispose", Map::new());
        Ok(())
    }
}

impl RemoteObject for JsHandle {
    fn channel(&self) -> &ChannelOwner { &self.channel }
    fn channel_mut(&mut self) -> &mut ChannelOwner { &mut self.channel }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Initializer {
    preview: String
}

#[derive(Deserialize)]
struct Property {
    name: String,
    value: OnlyGuid
}

pub(crate) mod ser {
    use crate::imp::{
        core::{Guid, OnlyGuid},
        prelude::*
    };
    use itertools::Itertools;
    use serde::ser;
    use std::{cell::RefCell, mem, rc::Rc};

    #[derive(Debug, thiserror::Error)]
    pub(crate) enum Error {
        #[error("{0:}")]
        Msg(String),
        #[error("Couldn't construct map from odd number of values")]
        OddMap,
        #[error("Key must be string")]
        InvalidKey,
        #[error("Not supported")]
        NotSupported,
        #[error("Failed to serialize JsHandle")]
        JsHandle
    }

    impl serde::ser::Error for Error {
        fn custom<T>(msg: T) -> Self
        where
            T: std::fmt::Display
        {
            Self::Msg(msg.to_string())
        }
    }

    #[derive(Clone, Default)]
    pub(crate) struct Serializer {
        handles: Rc<RefCell<Vec<OnlyGuid>>>,

        seq: Vec<Seq>,
        t: Vec<TupleVariant>,
        om: Vec<ObjectM>,
        os: Vec<ObjectS>,
        s: Vec<StructVariant>
    }

    pub(crate) fn to_value<T>(x: &T) -> Result<Value, Error>
    where
        T: Serialize
    {
        let mut serializer = Serializer::default();
        x.serialize(&mut serializer)
    }

    impl<'a> ser::Serializer for &'a mut Serializer {
        type Ok = Value;
        type Error = Error;

        type SerializeSeq = &'a mut Seq;
        type SerializeTuple = &'a mut Seq;
        type SerializeTupleStruct = &'a mut Seq;
        type SerializeTupleVariant = &'a mut TupleVariant;
        type SerializeMap = &'a mut ObjectM;
        type SerializeStruct = &'a mut ObjectS;
        type SerializeStructVariant = &'a mut StructVariant;

        fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
            let mut m = Map::new();
            m.insert("b".into(), v.into());
            Ok(m.into())
        }

        fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
            let mut m = Map::new();
            m.insert("n".into(), v.into());
            Ok(m.into())
        }
        fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
            self.serialize_i64(v.into())
        }
        fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
            self.serialize_i64(v.into())
        }
        fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
            self.serialize_i64(v.into())
        }

        fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
            let mut m = Map::new();
            m.insert("n".into(), v.into());
            Ok(m.into())
        }
        fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
            self.serialize_u64(v.into())
        }
        fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
            self.serialize_u64(v.into())
        }
        fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
            self.serialize_u64(v.into())
        }

        fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
            let mut m = Map::new();
            if v.is_nan() {
                m.insert("v".into(), "NaN".into())
            } else if v.is_infinite() {
                m.insert(
                    "v".into(),
                    if v.is_sign_negative() {
                        "-Infinity"
                    } else {
                        "Infinity"
                    }
                    .into()
                )
            } else if v.is_sign_negative() && v == -0.0 {
                m.insert("v".into(), "-0".into())
            } else {
                m.insert("n".into(), v.into())
            };
            Ok(m.into())
        }
        fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> { Ok(f64::from(v).into()) }

        fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
            let mut m = Map::new();
            m.insert("s".into(), v.into());
            Ok(m.into())
        }
        fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> {
            let mut m = Map::new();
            m.insert("s".into(), v.to_string().into());
            Ok(m.into())
        }

        fn serialize_bytes(self, _v: &[u8]) -> Result<Self::Ok, Self::Error> {
            Err(Error::NotSupported)
        }

        fn serialize_none(self) -> Result<Self::Ok, Self::Error> { self.serialize_unit() }
        fn serialize_some<T>(self, v: &T) -> Result<Self::Ok, Self::Error>
        where
            T: ?Sized + Serialize
        {
            v.serialize(self)
        }
        fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
            let mut m = Map::new();
            m.insert("v".into(), "undefined".into());
            Ok(m.into())
        }
        fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
            self.serialize_unit()
        }

        fn serialize_unit_variant(
            self,
            _name: &'static str,
            _variant_index: u32,
            variant: &'static str
        ) -> Result<Self::Ok, Self::Error> {
            self.serialize_str(variant)
        }

        fn serialize_newtype_struct<T>(
            self,
            _name: &'static str,
            value: &T
        ) -> Result<Self::Ok, Self::Error>
        where
            T: ?Sized + Serialize
        {
            value.serialize(self)
        }

        fn serialize_newtype_variant<T>(
            self,
            _name: &'static str,
            _variant_index: u32,
            variant: &'static str,
            value: &T
        ) -> Result<Self::Ok, Self::Error>
        where
            T: ?Sized + Serialize
        {
            let mut inner = Map::new();
            inner.insert(variant.into(), value.serialize(self)?);
            let mut m = Map::new();
            m.insert("o".into(), inner.into());
            Ok(m.into())
        }

        fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
            self.seq.push(Seq::new(self.clone()));
            Ok(self.seq.last_mut().unwrap())
        }

        fn serialize_tuple(self, len: usize) -> Result<Self::SerializeSeq, Self::Error> {
            self.serialize_seq(Some(len))
        }

        fn serialize_tuple_struct(
            self,
            _name: &'static str,
            len: usize
        ) -> Result<Self::SerializeSeq, Self::Error> {
            self.serialize_seq(Some(len))
        }

        fn serialize_tuple_variant(
            self,
            _name: &'static str,
            _variant_index: u32,
            variant: &'static str,
            _len: usize
        ) -> Result<Self::SerializeTupleVariant, Self::Error> {
            self.t.push(TupleVariant::new(self.clone(), variant));
            Ok(self.t.last_mut().unwrap())
        }

        fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
            self.om.push(ObjectM::new(self.clone()));
            Ok(self.om.last_mut().unwrap())
        }

        fn serialize_struct(
            self,
            name: &'static str,
            len: usize
        ) -> Result<Self::SerializeStruct, Self::Error> {
            self.os.push(ObjectS::new(self.clone(), name));
            Ok(self.os.last_mut().unwrap())
        }

        fn serialize_struct_variant(
            self,
            _name: &'static str,
            _variant_index: u32,
            variant: &'static str,
            _len: usize
        ) -> Result<Self::SerializeStructVariant, Self::Error> {
            self.s.push(StructVariant::new(self.clone(), variant));
            Ok(self.s.last_mut().unwrap())
        }
    }

    #[derive(Clone)]
    pub(crate) struct Seq {
        values: Vec<Value>,
        prime: Serializer
    }

    impl Seq {
        fn new(prime: Serializer) -> Self {
            Self {
                values: Vec::new(),
                prime
            }
        }
    }

    impl<'a> ser::SerializeSeq for &'a mut Seq {
        type Ok = Value;
        type Error = Error;

        fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
        where
            T: ?Sized + Serialize
        {
            self.values.push(value.serialize(&mut self.prime)?);
            Ok(())
        }

        fn end(self) -> Result<Self::Ok, Self::Error> {
            let mut m = Map::new();
            let mut vs = Vec::new();
            mem::swap(&mut self.values, &mut vs);
            m.insert("a".into(), vs.into());
            Ok(m.into())
        }
    }

    impl<'a> ser::SerializeTuple for &'a mut Seq {
        type Ok = Value;
        type Error = Error;

        fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
        where
            T: ?Sized + Serialize
        {
            self.values.push(value.serialize(&mut self.prime)?);
            Ok(())
        }

        fn end(self) -> Result<Self::Ok, Self::Error> {
            let mut m = Map::new();
            let mut vs = Vec::new();
            mem::swap(&mut self.values, &mut vs);
            m.insert("a".into(), vs.into());
            Ok(m.into())
        }
    }

    impl<'a> ser::SerializeTupleStruct for &'a mut Seq {
        type Ok = Value;
        type Error = Error;

        fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
        where
            T: ?Sized + Serialize
        {
            self.values.push(value.serialize(&mut self.prime)?);
            Ok(())
        }

        fn end(self) -> Result<Self::Ok, Self::Error> {
            let mut m = Map::new();
            let mut vs = Vec::new();
            mem::swap(&mut self.values, &mut vs);
            m.insert("a".into(), vs.into());
            Ok(m.into())
        }
    }

    #[derive(Clone)]
    pub(crate) struct TupleVariant {
        values: Vec<Value>,
        variant: &'static str,
        prime: Serializer
    }

    impl TupleVariant {
        fn new(prime: Serializer, variant: &'static str) -> Self {
            Self {
                values: Vec::new(),
                variant,
                prime
            }
        }
    }

    impl<'a> ser::SerializeTupleVariant for &'a mut TupleVariant {
        type Ok = Value;
        type Error = Error;

        fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
        where
            T: ?Sized + Serialize
        {
            self.values.push(value.serialize(&mut self.prime)?);
            Ok(())
        }

        fn end(self) -> Result<Self::Ok, Self::Error> {
            let mut inner = Map::new();
            let a = {
                let mut a = Map::new();
                let mut vs = Vec::new();
                mem::swap(&mut self.values, &mut vs);
                a.insert("a".into(), vs.into());
                a
            };
            inner.insert(self.variant.into(), a.into());
            let mut o = Map::new();
            o.insert("o".into(), inner.into());
            Ok(o.into())
        }
    }

    #[derive(Clone)]
    pub(crate) struct ObjectS {
        name: &'static str,
        map: Map<String, Value>,
        prime: Serializer,
        guid: Option<Str<Guid>>
    }

    #[derive(Clone)]
    pub(crate) struct ObjectM {
        values: Vec<Value>,
        prime: Serializer
    }

    impl ObjectS {
        fn new(prime: Serializer, name: &'static str) -> Self {
            Self {
                name,
                prime,
                map: Map::new(),
                guid: None
            }
        }
    }

    impl ObjectM {
        fn new(prime: Serializer) -> Self {
            Self {
                prime,
                values: Vec::new()
            }
        }
    }

    impl<'a> ser::SerializeStruct for &'a mut ObjectS {
        type Ok = Value;
        type Error = Error;

        fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
        where
            T: ?Sized + Serialize
        {
            let v = value.serialize(&mut self.prime)?;
            if self.name == "4a9c3811-6f00-49e5-8a81-939f932d9061" && key == "guid" {
                let g = match v {
                    Value::String(s) => Str::validate(s).unwrap(),
                    _ => return Err(Error::JsHandle)
                };
                self.guid = Some(g);
                return Ok(());
            }
            self.map.insert(key.into(), v);
            Ok(())
        }

        fn end(self) -> Result<Self::Ok, Self::Error> {
            if self.name == "4a9c3811-6f00-49e5-8a81-939f932d9061" {
                unimplemented!()
            } else {
                let mut o = Map::new();
                let mut m = Map::new();
                mem::swap(&mut self.map, &mut m);
                o.insert("o".into(), m.into());
                Ok(o.into())
            }
        }
    }

    impl<'a> ser::SerializeMap for &'a mut ObjectM {
        type Ok = Value;
        type Error = Error;

        fn serialize_key<T>(&mut self, key: &T) -> Result<(), Self::Error>
        where
            T: ?Sized + Serialize
        {
            self.values.push(key.serialize(&mut self.prime)?);
            Ok(())
        }

        fn serialize_value<T>(&mut self, value: &T) -> Result<(), Self::Error>
        where
            T: ?Sized + Serialize
        {
            self.values.push(value.serialize(&mut self.prime)?);
            Ok(())
        }

        fn end(self) -> Result<Self::Ok, Self::Error> {
            let mut vs = Vec::new();
            mem::swap(&mut self.values, &mut vs);
            if vs.len() % 2 == 1 {
                return Err(Error::OddMap);
            }
            let mut inner = Map::new();
            vs.into_iter().chunks(2).into_iter().try_for_each(
                |mut kv| -> Result<(), Self::Error> {
                    let k = kv.next().unwrap();
                    let v = kv.next().unwrap();
                    let key = match k {
                        Value::String(s) => s,
                        _ => return Err(Error::InvalidKey)
                    };
                    inner.insert(key.into(), v);
                    Ok(())
                }
            )?;
            let mut m = Map::new();
            m.insert("o".into(), inner.into());
            Ok(m.into())
        }
    }

    #[derive(Clone)]
    pub(crate) struct StructVariant {
        m: Map<String, Value>,
        variant: &'static str,
        prime: Serializer
    }

    impl StructVariant {
        fn new(prime: Serializer, variant: &'static str) -> Self {
            Self {
                m: Map::new(),
                variant,
                prime
            }
        }
    }

    impl<'a> ser::SerializeStructVariant for &'a mut StructVariant {
        type Ok = Value;
        type Error = Error;

        fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
        where
            T: ?Sized + Serialize
        {
            self.m.insert(key.into(), value.serialize(&mut self.prime)?);
            Ok(())
        }

        fn end(self) -> Result<Self::Ok, Self::Error> {
            let mut inner = Map::new();
            let m = {
                let mut m = Map::new();
                let mut v = Map::new();
                mem::swap(&mut self.m, &mut v);
                m.insert("o".into(), v.into());
                m
            };
            inner.insert(self.variant.into(), m.into());
            let mut o = Map::new();
            o.insert("o".into(), inner.into());
            Ok(o.into())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn r#struct() {
            #[derive(Serialize)]
            struct Test {
                int: u32,
                seq: Vec<&'static str>
            }

            let test = Test {
                int: 1,
                seq: vec!["a", "b"]
            };
            let expected = r#"{"o":{"int":{"n":1},"seq":{"a": [{"s":"a"},{"s":"b"}]}}}"#;
            let v: Value = serde_json::from_str(expected).unwrap();
            assert_eq!(to_value(&test).unwrap(), v);
        }

        #[test]
        fn r#enum() {
            #[derive(Serialize)]
            enum E {
                Unit,
                Newtype(u32),
                Tuple(u32, u32),
                Struct { a: u32 }
            }

            let u = E::Unit;
            let expected = r#"{"s":"Unit"}"#;
            let v: Value = serde_json::from_str(expected).unwrap();
            assert_eq!(to_value(&u).unwrap(), v);

            let u = E::Newtype(1);
            let expected = r#"{"o":{"Newtype":{"n":1}}}"#;
            let v: Value = serde_json::from_str(expected).unwrap();
            assert_eq!(to_value(&u).unwrap(), v);

            let u = E::Tuple(1, 2);
            let expected = r#"{"o":{"Tuple":{"a":[{"n":1},{"n":2}]}}}"#;
            let v: Value = serde_json::from_str(expected).unwrap();
            assert_eq!(to_value(&u).unwrap(), v);

            let u = E::Struct { a: 1 };
            let expected = r#"{"o":{"Struct":{"o":{"a":{"n":1}}}}}"#;
            let v: Value = serde_json::from_str(expected).unwrap();
            assert_eq!(to_value(&u).unwrap(), v);
        }
    }
}
