use crate::public_interface::constants::*;
use crate::public_interface::{PublicG1Api, G1Api, PublicG2Api, G2Api};
use crate::errors::ApiError;

use num_bigint::BigUint;

use crate::test::parsers::*;
use super::call_pairing_engine;

use crate::test::g1_ops;
use crate::test::g2_ops;

pub(crate) fn assemble_single_curve_params(curve: JsonBls12PairingCurveParameters, pairs: usize, check_subgroup: bool) -> Result<Vec<u8>, ApiError>  {
    let curve_clone = curve.clone();
    assert!(pairs % 2 == 0);
    // - Curve type
    // - Lengths of modulus (in bytes)
    // - Field modulus
    // - Curve A
    // - Curve B
    // - non-residue for Fp2
    // - non-residue for Fp6
    // - twist type M/D
    // - parameter X
    // - sign of X
    // - number of pairs
    // - list of encoded pairs

    // first determine the length of the modulus
    let modulus = curve.q;
    let modulus_length = modulus.clone().to_bytes_be().len();

    let curve_type = vec![BLS12];
    let modulus_len_encoded = vec![modulus_length as u8];
    let modulus_encoded = pad_for_len_be(modulus.clone().to_bytes_be(), modulus_length);

    let a_encoded = pad_for_len_be(curve.a.to_bytes_be(), modulus_length);
    let b_encoded = pad_for_len_be(curve.b.to_bytes_be(), modulus_length);

    let fp2_nonres_encoded = {
        let (mut nonres, is_positive) = curve.non_residue;
        if !is_positive {
            nonres = modulus.clone() - nonres;
        }
        pad_for_len_be(nonres.to_bytes_be(), modulus_length)
    };

    let fp6_nonres_encoded_c0 = {
        let (mut nonres, is_positive) = curve.quadratic_non_residue_0;
        if !is_positive {
            nonres = modulus.clone() - nonres;
        }
        pad_for_len_be(nonres.to_bytes_be(), modulus_length)
    };

    let fp6_nonres_encoded_c1 = {
        let (mut nonres, is_positive) = curve.quadratic_non_residue_1;
        if !is_positive {
            nonres = modulus.clone() - nonres;
        }
        pad_for_len_be(nonres.to_bytes_be(), modulus_length)
    };

    let twist_type = if curve.is_d_type { vec![TWIST_TYPE_D] } else { vec![TWIST_TYPE_M] };

    let (x_decoded, x_is_positive) = curve.x;
    let x_sign = if x_is_positive { vec![0u8] } else { vec![1u8] };
    let x_encoded = x_decoded.to_bytes_be();
    let x_length = vec![x_encoded.len() as u8];

    // now we make two random scalars and do scalar multiplications in G1 and G2 to get pairs that should
    // at the end of the day pair to identity element

    let group_size = curve.r;
    let group_size_encoded = group_size.clone().to_bytes_be();
    let group_size_length = group_size_encoded.len();
    let group_len_encoded = vec![group_size_length as u8];

    // first parse generators
    // g1 generator
    let g1_x = curve.g1_x;
    let g1_x = pad_for_len_be(g1_x.to_bytes_be(), modulus_length);
    let g1_y = curve.g1_y;
    let g1_y = pad_for_len_be(g1_y.to_bytes_be(), modulus_length);

    // g2 generator
    let g2_x_0 = curve.g2_x_0;
    let g2_x_1 = curve.g2_x_1;

    let g2_y_0 = curve.g2_y_0;
    let g2_y_1 = curve.g2_y_1;

    let num_pairs = vec![pairs as u8];

    let mut g1_encodings = vec![];
    let mut g2_encodings = vec![];

    let g2_generator_encoding = {
        let mut g2_generator_encoding = vec![];
        g2_generator_encoding.extend(pad_for_len_be(g2_x_0.to_bytes_be(), modulus_length));
        g2_generator_encoding.extend(pad_for_len_be(g2_x_1.to_bytes_be(), modulus_length));
        g2_generator_encoding.extend(pad_for_len_be(g2_y_0.to_bytes_be(), modulus_length));
        g2_generator_encoding.extend(pad_for_len_be(g2_y_1.to_bytes_be(), modulus_length));

        g2_generator_encoding
    };

    // for multiplications we use the public API itself - just construct the corresponding G1
    // multiplication API. Leave G2 as generators for now

    use rand::{Rng, SeedableRng};
    use rand_xorshift::XorShiftRng;

    let rng = &mut XorShiftRng::from_seed([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);

    {
        fn make_random_scalar<R: Rng>(rng: &mut R, group_size_length: usize, group_size: &BigUint) -> BigUint {
            let random_scalar_bytes: Vec<u8> = (0..group_size_length).map(|_| rng.gen()).collect();
            let random_scalar = BigUint::from_bytes_be(&random_scalar_bytes[..]);
            let random_scalar = random_scalar % group_size;

            random_scalar
        }

        for _ in 0..(pairs/2) {
            // - Multiplication API signature
            // - Lengths of modulus (in bytes)
            // - Field modulus
            // - Curve A
            // - Curve B
            // - Length of a scalar field (curve order) (in bytes)
            // - Curve order
            // - X
            // - Y
            // - Scalar
            
            let r1 = make_random_scalar(rng, group_size_length, &group_size);
            let r2 = make_random_scalar(rng, group_size_length, &group_size);
            let r3 = (r1.clone() * &r2) % &group_size;
            let r3 = group_size.clone() - r3;

            // pair (g1^r1, g2^r2)*(g1^(-r1*r2), g2)
            let (g1_common_bytes, _, _) = g1_ops::bls12::assemble_single_curve_params(curve_clone.clone());
            let (g2_common_bytes, _, _) = g2_ops::bls12::assemble_single_curve_params(curve_clone.clone());

            let g1_encoded_0 = {
                let mut mul_calldata = vec![];
                mul_calldata.extend(g1_common_bytes.clone());
                mul_calldata.extend_from_slice(&g1_x[..]);
                mul_calldata.extend_from_slice(&g1_y[..]);
                mul_calldata.extend(pad_for_len_be(r1.to_bytes_be(), group_size_length));

                let g1 = PublicG1Api::mul_point(&mul_calldata[..])?;

                g1
            };

            let g2_encoded_0 = {
                let mut mul_calldata = vec![];
                mul_calldata.extend(g2_common_bytes.clone());
                mul_calldata.extend(g2_generator_encoding.clone());
                mul_calldata.extend(pad_for_len_be(r2.to_bytes_be(), group_size_length));

                let g2 = PublicG2Api::mul_point(&mul_calldata[..])?;

                g2
            };

            let g1_encoded_1 = {
                let mut mul_calldata = vec![];
                mul_calldata.extend(g1_common_bytes.clone());
                mul_calldata.extend_from_slice(&g1_x[..]);
                mul_calldata.extend_from_slice(&g1_y[..]);
                mul_calldata.extend(pad_for_len_be(r3.to_bytes_be(), group_size_length));

                let g1 = PublicG1Api::mul_point(&mul_calldata[..])?;

                g1
            };

            let g2_encoded_1 = g2_generator_encoding.clone();

            g1_encodings.push(g1_encoded_0);
            g1_encodings.push(g1_encoded_1);

            g2_encodings.push(g2_encoded_0);
            g2_encodings.push(g2_encoded_1);
        }
    }

    let mut calldata = vec![];
    calldata.extend(curve_type.into_iter());
    calldata.extend(modulus_len_encoded.into_iter());
    calldata.extend(modulus_encoded.into_iter());
    calldata.extend(a_encoded.into_iter());
    calldata.extend(b_encoded.into_iter());
    calldata.extend(group_len_encoded.into_iter());
    calldata.extend(group_size_encoded.into_iter());
    calldata.extend(fp2_nonres_encoded.into_iter());
    calldata.extend(fp6_nonres_encoded_c0.into_iter());
    calldata.extend(fp6_nonres_encoded_c1.into_iter());
    calldata.extend(twist_type.into_iter());
    calldata.extend(x_length.into_iter());
    calldata.extend(x_encoded.into_iter());
    calldata.extend(x_sign.into_iter());
    calldata.extend(num_pairs.into_iter());
    for (g1, g2) in g1_encodings.into_iter().zip(g2_encodings.into_iter()) {
        if check_subgroup {
            calldata.extend(vec![1u8]);
        } else {
            calldata.extend(vec![0u8]);
        }
        calldata.extend(g1.into_iter());
        if check_subgroup {
            calldata.extend(vec![1u8]);
        } else {
            calldata.extend(vec![0u8]);
        }
        calldata.extend(g2.into_iter());
    }

    Ok(calldata)
}

// #[test]
// fn test_single() {
//     let calldata = assemble_single();
//     let result = call_pairing_engine(&calldata[..]);
//     assert!(result.is_ok());

//     let result = result.unwrap()[0];
//     assert!(result == 1u8);
// }

#[test]
fn test_bls12_pairings_from_vectors() {
    let curves = read_dir_and_grab_curves("src/test/test_vectors/bls12/");
    assert!(curves.len() != 0);
    for (curve, _) in curves.into_iter() {
        let calldata = assemble_single_curve_params(curve, 2, true).unwrap();
        let result = call_pairing_engine(&calldata[..]);
        if !result.is_ok() {
            println!("Error {}", result.err().unwrap());
        } else {
            let result = result.unwrap()[0];
            assert!(result == 1u8);
        }
    }
}

#[test]
#[ignore]
fn test_bench_bls12_pairings_from_vectors() {
    let curves = read_dir_and_grab_curves("src/test/test_vectors/bls12/");
    assert!(curves.len() != 0);
    for (curve, _) in curves.into_iter() {
        let calldata = assemble_single_curve_params(curve, 2, true).unwrap();
        let start = std::time::Instant::now();
        let result = call_pairing_engine(&calldata[..]);
        println!("Taken {:?}", start.elapsed());

        if !result.is_ok() {
            println!("Error {}", result.err().unwrap());
        } else {
            let result = result.unwrap()[0];
            assert!(result == 1u8);
        }
    }
}

extern crate hex;
extern crate csv;

use hex::{encode};
use csv::{Writer};

#[test]
#[ignore]
fn dump_pairing_vectors() {
    let curves = read_dir_and_grab_curves::<JsonBls12PairingCurveParameters>("src/test/test_vectors/bls12/");
    assert!(curves.len() != 0);
    let mut writer = Writer::from_path("src/test/test_vectors/bls12/pairing.csv").expect("must open a test file");
    writer.write_record(&["input", "result"]).expect("must write header");
    for (curve, _) in curves.into_iter() {
        let mut input_data = vec![OPERATION_PAIRING];
        let calldata = assemble_single_curve_params(curve.clone(), 2, true).unwrap();
        input_data.extend(&calldata[1..]);
        let expected_result = vec![1u8];
        writer.write_record(&[
            prepend_0x(&encode(&input_data[..])), 
            prepend_0x(&encode(&expected_result[..]))],
        ).expect("must write a record");
    }
    writer.flush().expect("must finalize writing");
}

#[test]
#[ignore]
fn dump_fuzzing_vectors() {
    use std::io::Write;
    use std::fs::File;
    let curves = read_dir_and_grab_curves::<JsonBls12PairingCurveParameters>("src/test/test_vectors/bls12/");
    assert!(curves.len() != 0);
    
    // let mut writer = Writer::from_path("src/test/test_vectors/bls12/pairing.csv").expect("must open a test file");
    // writer.write_record(&["input", "result"]).expect("must write header");
    for (curve, _) in curves.into_iter() {
        let mut input_data = vec![OPERATION_PAIRING];
        let calldata = assemble_single_curve_params(curve.clone(), 2, true).unwrap();
        input_data.extend(calldata);
        let filename = hex::encode(&input_data);
        let mut f = File::create(&format!("src/test/test_vectors/bls12/fuzzing_corpus/{}", &filename[0..40])).unwrap();
        f.write_all(&mut input_data[..]).expect("must write");
    }
}

// use rust_test::Bencher;

// #[bench]
// fn bench_single(b: &mut Bencher) {
//     let calldata = assemble_single();
//     b.iter(|| {
//         call_pairing_engine(&calldata[..]).expect("must use");
//     });
// }

pub(crate) fn assemble_bls12_381(num_point_pairs: usize) -> Vec<u8> {
    /// - Curve type
    /// - Lengths of modulus (in bytes)
    /// - Field modulus
    /// - Curve A
    /// - Curve B
    // - non-residue for Fp2
    // - non-residue for Fp6
    // - twist type M/D
    // - parameter X
    // - sign of X
    // - number of pairs
    // - list of encoded pairs
    use num_traits::Num;
    let modulus_length = 48;
    let modulus = BigUint::from_str_radix("4002409555221667393417789825735904156556882819939007885332058136124031650490837864442687629129015664037894272559787", 10).unwrap();
    let curve_type = vec![BLS12];
    let modulus_len_encoded = vec![modulus_length as u8];
    let modulus_encoded = pad_for_len_be(modulus.clone().to_bytes_be(), modulus_length);
    let a_encoded = pad_for_len_be(BigUint::from(0u64).to_bytes_be(), modulus_length);
    let b_encoded = pad_for_len_be(BigUint::from(4u64).to_bytes_be(), modulus_length);
    let group_order_len = 32;
    let group_order = BigUint::from_str_radix("52435875175126190479447740508185965837690552500527637822603658699938581184513", 10).unwrap();
    let group_order_encoding = pad_for_len_be(group_order.to_bytes_be(), group_order_len);
    let minus_one = modulus.clone() - BigUint::from(1u64);
    let fp2_nonres_encoded = pad_for_len_be(minus_one.to_bytes_be(), modulus_length);
    let fp6_nonres_encoded_c0 = pad_for_len_be(BigUint::from(1u64).to_bytes_be(), modulus_length);
    let fp6_nonres_encoded_c1 = pad_for_len_be(BigUint::from(1u64).to_bytes_be(), modulus_length);
    let twist_type = vec![TWIST_TYPE_M]; // M
    let x_length = vec![8u8];
    let x_encoded = pad_for_len_be(BigUint::from(0xd201000000010000 as u64).to_bytes_be(), 8); 
    let x_sign = vec![SIGN_MINUS]; // negative x
    let num_pairs = vec![num_point_pairs as u8];
    // first pair
    let p_x = BigUint::from_str_radix("3685416753713387016781088315183077757961620795782546409894578378688607592378376318836054947676345821548104185464507", 10).unwrap().to_bytes_be();
    let p_y = BigUint::from_str_radix("1339506544944476473020471379941921221584933875938349620426543736416511423956333506472724655353366534992391756441569", 10).unwrap().to_bytes_be();
    let mut g1_0_encoding: Vec<u8> = vec![];
    g1_0_encoding.push(1u8);
    g1_0_encoding.extend(pad_for_len_be(p_x.clone(), modulus_length).into_iter());
    g1_0_encoding.extend(pad_for_len_be(p_y, modulus_length).into_iter());

    let q_x_0 = BigUint::from_str_radix("352701069587466618187139116011060144890029952792775240219908644239793785735715026873347600343865175952761926303160", 10).unwrap().to_bytes_be();
    let q_x_1 = BigUint::from_str_radix("3059144344244213709971259814753781636986470325476647558659373206291635324768958432433509563104347017837885763365758", 10).unwrap().to_bytes_be();
    let q_y_0 = BigUint::from_str_radix("1985150602287291935568054521177171638300868978215655730859378665066344726373823718423869104263333984641494340347905", 10).unwrap().to_bytes_be();
    let q_y_1 = BigUint::from_str_radix("927553665492332455747201965776037880757740193453592970025027978793976877002675564980949289727957565575433344219582", 10).unwrap().to_bytes_be();
    let mut g2_0_encoding = vec![];
    g2_0_encoding.push(1u8);
    g2_0_encoding.extend(pad_for_len_be(q_x_0.clone(), modulus_length).into_iter());
    g2_0_encoding.extend(pad_for_len_be(q_x_1.clone(), modulus_length).into_iter());
    g2_0_encoding.extend(pad_for_len_be(q_y_0.clone(), modulus_length).into_iter());
    g2_0_encoding.extend(pad_for_len_be(q_y_1.clone(), modulus_length).into_iter());

    // second pair 
    let y = modulus.clone() - BigUint::from_str_radix("1339506544944476473020471379941921221584933875938349620426543736416511423956333506472724655353366534992391756441569", 10).unwrap();

    let mut g1_1_encoding: Vec<u8> = vec![];
    g1_1_encoding.push(1u8);
    g1_1_encoding.extend(pad_for_len_be(p_x.clone(), modulus_length).into_iter());
    g1_1_encoding.extend(pad_for_len_be(y.to_bytes_be(), modulus_length).into_iter());

    let g2_1_encoding = g2_0_encoding.clone();

    let mut calldata = vec![];
    calldata.extend(curve_type.into_iter());
    calldata.extend(modulus_len_encoded.into_iter());
    calldata.extend(modulus_encoded.into_iter());
    calldata.extend(a_encoded.into_iter());
    calldata.extend(b_encoded.into_iter());
    calldata.extend(vec![group_order_len as u8]);
    calldata.extend(group_order_encoding.into_iter());
    calldata.extend(fp2_nonres_encoded.into_iter());
    calldata.extend(fp6_nonres_encoded_c0.into_iter());
    calldata.extend(fp6_nonres_encoded_c1.into_iter());
    calldata.extend(twist_type.into_iter());
    calldata.extend(x_length.into_iter());
    calldata.extend(x_encoded.into_iter());
    calldata.extend(x_sign.into_iter());
    calldata.extend(num_pairs.into_iter());

    for i in 0..num_point_pairs {
        if i % 2 == 0 {
            calldata.extend(g1_0_encoding.clone().into_iter());
            calldata.extend(g2_0_encoding.clone().into_iter());
        } else {
            calldata.extend(g1_1_encoding.clone().into_iter());
            calldata.extend(g2_1_encoding.clone().into_iter());
        }
    }

    calldata
}

pub(crate) fn assemble_bls12_377(num_point_pairs: usize) -> Vec<u8> {
    /// - Curve type
    /// - Lengths of modulus (in bytes)
    /// - Field modulus
    /// - Curve A
    /// - Curve B
    // - non-residue for Fp2
    // - non-residue for Fp6
    // - twist type M/D
    // - parameter X
    // - sign of X
    // - number of pairs
    // - list of encoded pairs
    use num_traits::Num;
    let modulus_length = 48;
    let modulus = BigUint::from_str_radix("258664426012969094010652733694893533536393512754914660539884262666720468348340822774968888139573360124440321458177", 10).unwrap();
    let curve_type = vec![BLS12];
    let modulus_len_encoded = vec![modulus_length as u8];
    let modulus_encoded = pad_for_len_be(modulus.clone().to_bytes_be(), modulus_length);
    let a_encoded = pad_for_len_be(BigUint::from(0u64).to_bytes_be(), modulus_length);
    let b_encoded = pad_for_len_be(BigUint::from(1u64).to_bytes_be(), modulus_length);
    let group_order_len = 32;
    let group_order = BigUint::from_str_radix("8444461749428370424248824938781546531375899335154063827935233455917409239041", 10).unwrap();
    let group_order_encoding = pad_for_len_be(group_order.to_bytes_be(), group_order_len);
    let minus_five = modulus.clone() - BigUint::from(5u64);
    let fp2_nonres_encoded = pad_for_len_be(minus_five.to_bytes_be(), modulus_length);
    let fp6_nonres_encoded_c0 = pad_for_len_be(BigUint::from(0u64).to_bytes_be(), modulus_length);
    let fp6_nonres_encoded_c1 = pad_for_len_be(BigUint::from(1u64).to_bytes_be(), modulus_length);
    let twist_type = vec![TWIST_TYPE_D];
    let x_length = vec![8u8];
    let x_encoded = pad_for_len_be(BigUint::from(0x8508c00000000001 as u64).to_bytes_be(), 8); 
    let x_sign = vec![SIGN_PLUS];
    let num_pairs = vec![num_point_pairs as u8];
    // first pair
    let p_x = BigUint::from_str_radix("008848defe740a67c8fc6225bf87ff5485951e2caa9d41bb188282c8bd37cb5cd5481512ffcd394eeab9b16eb21be9ef", 16).unwrap().to_bytes_be();
    let p_y = BigUint::from_str_radix("01914a69c5102eff1f674f5d30afeec4bd7fb348ca3e52d96d182ad44fb82305c2fe3d3634a9591afd82de55559c8ea6", 16).unwrap().to_bytes_be();
    let mut g1_0_encoding: Vec<u8> = vec![];
    g1_0_encoding.push(1u8);
    g1_0_encoding.extend(pad_for_len_be(p_x.clone(), modulus_length).into_iter());
    g1_0_encoding.extend(pad_for_len_be(p_y, modulus_length).into_iter());

    let q_x_0 = BigUint::from_str_radix("018480be71c785fec89630a2a3841d01c565f071203e50317ea501f557db6b9b71889f52bb53540274e3e48f7c005196", 16).unwrap().to_bytes_be();
    let q_x_1 = BigUint::from_str_radix("00ea6040e700403170dc5a51b1b140d5532777ee6651cecbe7223ece0799c9de5cf89984bff76fe6b26bfefa6ea16afe", 16).unwrap().to_bytes_be();
    let q_y_0 = BigUint::from_str_radix("00690d665d446f7bd960736bcbb2efb4de03ed7274b49a58e458c282f832d204f2cf88886d8c7c2ef094094409fd4ddf", 16).unwrap().to_bytes_be();
    let q_y_1 = BigUint::from_str_radix("00f8169fd28355189e549da3151a70aa61ef11ac3d591bf12463b01acee304c24279b83f5e52270bd9a1cdd185eb8f93", 16).unwrap().to_bytes_be();

    let mut g2_0_encoding = vec![];
    g2_0_encoding.push(1u8);
    g2_0_encoding.extend(pad_for_len_be(q_x_0.clone(), modulus_length).into_iter());
    g2_0_encoding.extend(pad_for_len_be(q_x_1.clone(), modulus_length).into_iter());
    g2_0_encoding.extend(pad_for_len_be(q_y_0.clone(), modulus_length).into_iter());
    g2_0_encoding.extend(pad_for_len_be(q_y_1.clone(), modulus_length).into_iter());

    // second pair 
    let y = modulus.clone() - BigUint::from_str_radix("01914a69c5102eff1f674f5d30afeec4bd7fb348ca3e52d96d182ad44fb82305c2fe3d3634a9591afd82de55559c8ea6", 16).unwrap();

    let mut g1_1_encoding: Vec<u8> = vec![];
    g1_1_encoding.push(1u8);
    g1_1_encoding.extend(pad_for_len_be(p_x.clone(), modulus_length).into_iter());
    g1_1_encoding.extend(pad_for_len_be(y.to_bytes_be(), modulus_length).into_iter());

    let g2_1_encoding = g2_0_encoding.clone();

    let mut calldata = vec![];
    calldata.extend(curve_type.into_iter());
    calldata.extend(modulus_len_encoded.into_iter());
    calldata.extend(modulus_encoded.into_iter());
    calldata.extend(a_encoded.into_iter());
    calldata.extend(b_encoded.into_iter());
    calldata.extend(vec![group_order_len as u8]);
    calldata.extend(group_order_encoding.into_iter());
    calldata.extend(fp2_nonres_encoded.into_iter());
    calldata.extend(fp6_nonres_encoded_c0.into_iter());
    calldata.extend(fp6_nonres_encoded_c1.into_iter());
    calldata.extend(twist_type.into_iter());
    calldata.extend(x_length.into_iter());
    calldata.extend(x_encoded.into_iter());
    calldata.extend(x_sign.into_iter());
    calldata.extend(num_pairs.into_iter());

    for i in 0..num_point_pairs {
        if i % 2 == 0 {
            calldata.extend(g1_0_encoding.clone().into_iter());
            calldata.extend(g2_0_encoding.clone().into_iter());
        } else {
            calldata.extend(g1_1_encoding.clone().into_iter());
            calldata.extend(g2_1_encoding.clone().into_iter());
        }
    }

    calldata
}

#[test]
fn test_call_public_api_on_bls12_381() {
    let calldata = assemble_bls12_381(4);
    use crate::public_interface::PairingApi;

    let result = crate::public_interface::PublicPairingApi::pair(&calldata).unwrap();
    assert!(result.len() == 1);
    assert!(result[0] == 1);
}

#[test]
fn test_call_public_api_on_bls12_377() {
    let calldata = assemble_bls12_377(4);
    use crate::public_interface::PairingApi;

    let result = crate::public_interface::PublicPairingApi::pair(&calldata).unwrap();
    assert!(result.len() == 1);
    assert!(result[0] == 1);
}

#[test]
// #[ignore]
fn test_print_bls12_381_test_vector() {
    let calldata = assemble_bls12_381(1);
    // ignore curve type
    println!("{}", hex::encode(&calldata[1..]));
}

fn strip_0x(string: &str) -> String {
    let string = string.trim();
    let mut string = string.to_ascii_lowercase().as_bytes().to_vec();
    if string.len() > 2 && string[0..1] == b"0"[..] && string[1..2] == b"x"[..] {
        string = string[2..].to_vec();
    }
    
    std::string::String::from_utf8(string).unwrap()
}

fn strip_0x_and_get_sign(string: &str) -> (String, bool) {
    let string = string.trim();
    let mut string = string.to_ascii_lowercase().as_bytes().to_vec();
    let mut positive = true;
    if string.len() > 1 && string[0..1] == b"-"[..] {
        string = string[1..].to_vec();
        positive = false;
    }
    if string.len() > 1 && string[0..1] == b"+"[..] {
        string = string[1..].to_vec();
    }
    if string.len() > 2 && string[0..1] == b"0"[..] && string[1..2] == b"x"[..] {
        string = string[2..].to_vec();
    }
    
    (std::string::String::from_utf8(string).unwrap(), positive)
}

fn strip_0x_and_pad(string: &str) -> String {
    let string = string.trim();
    let mut string = string.to_ascii_lowercase().as_bytes().to_vec();
    if string.len() > 2 && string[0..1] == b"0"[..] && string[1..2] == b"x"[..] {
        let mut string = string[2..].to_vec();
        if string.len() % 2 == 1 {
            string = {
                let mut res = "0".as_bytes().to_vec();
                res.extend(string.into_iter());

                res
            };
        }
        return std::string::String::from_utf8(string).unwrap();
    }
    if string.len() % 2 == 1 {
        string = {
            let mut res = "0".as_bytes().to_vec();
            res.extend(string.into_iter());

            res
        };
    }

    std::string::String::from_utf8(string).unwrap()
}