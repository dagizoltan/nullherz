
pub(crate) mod serde_arc {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::sync::Arc;

    pub fn serialize<T, S>(val: &Arc<T>, s: S) -> Result<S::Ok, S::Error>
    where
        T: Serialize,
        S: Serializer,
    {
        val.as_ref().serialize(s)
    }

    pub fn deserialize<'de, T, D>(d: D) -> Result<Arc<T>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        T::deserialize(d).map(Arc::new)
    }
}

pub(crate) mod serde_arc_vec {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::sync::Arc;

    pub fn serialize<T, S>(val: &Vec<Arc<T>>, s: S) -> Result<S::Ok, S::Error>
    where
        T: Serialize,
        S: Serializer,
    {
        let temp: Vec<&T> = val.iter().map(|arc| arc.as_ref()).collect();
        temp.serialize(s)
    }

    pub fn deserialize<'de, T, D>(d: D) -> Result<Vec<Arc<T>>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        let temp: Vec<T> = Vec::deserialize(d)?;
        Ok(temp.into_iter().map(Arc::new).collect())
    }
}

