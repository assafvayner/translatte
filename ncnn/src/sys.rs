//! The subset of NCNN's C API (`src/c_api.h`) the safe wrapper uses, and one C++ function the C API
//! doesn't wrap.

use std::ffi::{c_char, c_int, c_void};

#[repr(C)]
pub struct OptionT {
    _private: [u8; 0],
}
#[repr(C)]
pub struct NetT {
    _private: [u8; 0],
}
#[repr(C)]
pub struct ExtractorT {
    _private: [u8; 0],
}
#[repr(C)]
pub struct MatT {
    _private: [u8; 0],
}
#[repr(C)]
pub struct AllocatorT {
    _private: [u8; 0],
}

pub type Option = *mut OptionT;
pub type Net = *mut NetT;
pub type Extractor = *mut ExtractorT;
pub type Mat = *mut MatT;
pub type Allocator = *mut AllocatorT;

extern "C" {
    pub fn ncnn_option_create() -> Option;
    pub fn ncnn_option_destroy(opt: Option);
    pub fn ncnn_option_set_num_threads(opt: Option, num_threads: c_int);
    pub fn ncnn_option_set_use_packing_layout(opt: Option, enable: c_int);
    pub fn ncnn_option_set_use_fp16_packed(opt: Option, enable: c_int);
    pub fn ncnn_option_set_use_fp16_storage(opt: Option, enable: c_int);
    pub fn ncnn_option_set_use_fp16_arithmetic(opt: Option, enable: c_int);
    pub fn ncnn_option_set_use_bf16_packed(opt: Option, enable: c_int);
    pub fn ncnn_option_set_use_bf16_storage(opt: Option, enable: c_int);

    pub fn ncnn_net_create() -> Net;
    pub fn ncnn_net_destroy(net: Net);
    pub fn ncnn_net_set_option(net: Net, opt: Option);
    pub fn ncnn_net_load_param(net: Net, path: *const c_char) -> c_int;
    pub fn ncnn_net_load_model(net: Net, path: *const c_char) -> c_int;

    pub fn ncnn_extractor_create(net: Net) -> Extractor;
    pub fn ncnn_extractor_destroy(ex: Extractor);
    pub fn ncnn_extractor_input(ex: Extractor, name: *const c_char, mat: Mat) -> c_int;
    pub fn ncnn_extractor_extract(ex: Extractor, name: *const c_char, mat: *mut Mat) -> c_int;

    pub fn ncnn_mat_create_3d(w: c_int, h: c_int, c: c_int, allocator: Allocator) -> Mat;
    pub fn ncnn_mat_destroy(mat: Mat);
    pub fn ncnn_mat_get_dims(mat: Mat) -> c_int;
    pub fn ncnn_mat_get_w(mat: Mat) -> c_int;
    pub fn ncnn_mat_get_h(mat: Mat) -> c_int;
    pub fn ncnn_mat_get_d(mat: Mat) -> c_int;
    pub fn ncnn_mat_get_c(mat: Mat) -> c_int;
    pub fn ncnn_mat_get_elemsize(mat: Mat) -> usize;
    pub fn ncnn_mat_get_elempack(mat: Mat) -> c_int;
    pub fn ncnn_mat_get_channel_data(mat: Mat, c: c_int) -> *mut c_void;

    /// `int ncnn::get_big_cpu_count()` from `src/cpu.h`, by its Itanium C++ ABI name.
    #[link_name = "_ZN4ncnn17get_big_cpu_countEv"]
    pub fn get_big_cpu_count() -> c_int;
}
