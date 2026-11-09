//! Pruebas de la ampliación de subred cuando el router queda fuera.

use smartpot_iot_host::subnet::prefix_including_gateway;

#[test]
fn keeps_a_normal_lease() {
    assert_eq!(
        prefix_including_gateway([192, 168, 1, 37], 24, [192, 168, 1, 1]),
        None
    );
    assert_eq!(
        prefix_including_gateway([10, 13, 37, 2], 24, [10, 13, 37, 1]),
        None
    );
    assert_eq!(
        prefix_including_gateway([10, 0, 0, 5], 0, [8, 8, 8, 8]),
        None
    );
}

#[test]
fn widens_a_host_route_to_include_the_gateway() {
    assert_eq!(
        prefix_including_gateway([10, 10, 0, 2], 32, [10, 10, 0, 1]),
        Some(30)
    );
    assert_eq!(
        prefix_including_gateway([10, 10, 0, 2], 30, [10, 10, 0, 9]),
        Some(28)
    );
    assert_eq!(
        prefix_including_gateway([10, 10, 0, 2], 24, [10, 10, 1, 1]),
        Some(23)
    );
}

#[test]
fn never_returns_a_prefix_longer_than_30() {
    assert_eq!(
        prefix_including_gateway([10, 10, 0, 2], 32, [10, 10, 0, 3]),
        Some(30)
    );
}

#[test]
fn treats_out_of_range_prefixes_as_32() {
    assert_eq!(
        prefix_including_gateway([10, 10, 0, 2], 40, [10, 10, 0, 1]),
        Some(30)
    );
}
