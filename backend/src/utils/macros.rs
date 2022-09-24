#[macro_export]
macro_rules! define_from {
    (
        $ty:ty {
            $(
                $key:ident = $value:ty
            ),* $(,)?
        }
    ) => {

        $(
            impl From<$value> for $ty {

                fn from(_: $value) -> $ty {
                    $ty::$key
                }

            }
        )*

    };
}

#[macro_export]
macro_rules! define_from_value {
    (
        $ty:ty {
            $(
                $key:ident = $value:ty
            ),* $(,)?
        }
    ) => {

        $(
            impl From<$value> for $ty {

                fn from(err: $value) -> $ty {
                    <$ty>::$key(err)
                }

            }
        )*

    };
}
