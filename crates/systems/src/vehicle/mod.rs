//! Applies vehicle boarding, seat, passenger, and camera-relevant transitions.
//!
//! `Vehicle_C.cpp`, `VehiclePassenger_C.cpp`, `Passenger.cpp`, and
//! `UnitVehicle_C.cpp` establish this stock behavior family.

mod animation;
mod passenger;
mod transition;
mod unit_vehicle_c;
mod vehicle_camera_c;
mod vehicle_passenger_c;
pub use animation::VehiclePassengerAnimationInput;
pub use transition::{
    VehiclePassengerPhase, VehiclePassengerTransition, VehicleTransitionInput,
    VehicleTransitionPose,
};

pub use vehicle_passenger_c::{
    VehicleSeatPose, vehicle_entry_target, vehicle_seat_attachment, vehicle_seat_transform,
};
