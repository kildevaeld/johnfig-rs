use super::Value;

pub fn merge(a: &mut Value, b: Value) {
    match (a, b) {
        (Value::Map(ref mut a), Value::Map(b)) => {
            for (k, v) in b.into_iter() {
                merge(a.entry(k).or_insert(Value::Option(None)), v);
            }
        }
        (Value::List(ref mut a), Value::List(b)) => {
            a.extend(b);
        }
        (Value::List(ref mut a), Value::Map(b)) => {
            a.extend([Value::Map(b)]);
        }
        (a, b) => *a = b,
    }
}
