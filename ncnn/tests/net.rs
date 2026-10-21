use std::path::PathBuf;

use ncnn::{Error, Net, Options};

fn data(name: &str) -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data")).join(name)
}

/// tiny.param: a 1x1 convolution to two channels (weights 2 and -1, biases 0.5 and 0), then ReLU.
fn tiny(fp16: bool) -> Net {
    let opts = Options { threads: 2, fp16 };
    Net::load(&data("tiny.param"), &data("tiny.bin"), &opts).unwrap()
}

#[test]
fn runs_the_tiny_model() {
    let net = tiny(false);
    // 3 values per channel, so NCNN pads each output channel to 4 (cstep) internally.
    let out = net
        .run("in0", &[-1.0, 0.0, 1.0], (1, 1, 3), "out0")
        .unwrap();
    assert_eq!(out.shape, vec![2, 1, 3]);
    assert_eq!(out.data, vec![0.0, 0.5, 2.5, 1.0, 0.0, 0.0]);
}

#[test]
fn runs_the_tiny_model_in_fp16() {
    let net = tiny(true);
    let out = net
        .run("in0", &[-1.0, 0.0, 1.0], (1, 1, 3), "out0")
        .unwrap();
    assert_eq!(out.shape, vec![2, 1, 3]);
    let want = [0.0, 0.5, 2.5, 1.0, 0.0, 0.0];
    for (g, w) in out.data.iter().zip(want) {
        assert!((g - w).abs() < 1e-2, "{:?}", out.data);
    }
}

#[test]
fn runs_repeatedly_with_other_shapes() {
    let net = tiny(false);
    for w in [1, 5, 16] {
        let input: Vec<f32> = (0..w * 2).map(|i| i as f32).collect();
        let out = net.run("in0", &input, (1, 2, w), "out0").unwrap();
        assert_eq!(out.shape, vec![2, 2, w]);
        let want: Vec<f32> = input.iter().map(|x| 2.0 * x + 0.5).collect();
        assert_eq!(&out.data[..w * 2], &want[..]);
    }
}

#[test]
fn reports_bad_inputs_and_files() {
    let net = tiny(false);
    assert!(matches!(
        net.run("in0", &[1.0, 2.0], (1, 1, 3), "out0"),
        Err(Error::Run(_))
    ));
    assert!(matches!(
        net.run("in0", &[1.0], (1, 1, 1), "nope"),
        Err(Error::Run(_))
    ));
    let opts = Options {
        threads: 1,
        fp16: false,
    };
    assert!(matches!(
        Net::load(&data("missing.param"), &data("tiny.bin"), &opts),
        Err(Error::Load(_))
    ));
}
