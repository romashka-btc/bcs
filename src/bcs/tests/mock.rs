use crate::{
    bcs::{
        tests::FieldMTConfig,
        transcript::{
            test_utils::check_commit_phase_correctness, NameSpace, SimulationTranscript, Transcript,
        },
        MTHashParameters,
    },
    iop::{
        message::{MessagesCollection, ProverRoundMessageInfo, RoundOracle, VerifierMessage},
        prover::IOPProver,
        verifier::IOPVerifier,
    },
    ldt::rl_ldt::{LinearCombinationLDT, LinearCombinationLDTParameters},
    test_utils::poseidon_parameters,
    Error,
};
use ark_bls12_381::fr::Fr;
use ark_crypto_primitives::merkle_tree::Config as MTConfig;
use ark_ff::{PrimeField, ToConstraintField};
use ark_ldt::{domain::Radix2CosetDomain, fri::FRIParameters};
use ark_poly::{univariate::DensePolynomial, UVPolynomial};
use ark_sponge::{poseidon::PoseidonSponge, Absorb, CryptographicSponge, FieldElementSize};
use ark_std::{marker::PhantomData, test_rng, vec, vec::Vec, One};

pub(crate) struct MockTestProver<F: PrimeField + Absorb> {
    _field: PhantomData<F>,
}

impl<F: PrimeField + Absorb> IOPProver<F> for MockTestProver<F> {
    type ProverParameter = ();
    type RoundOracleRefs = ();
    type PublicInput = ();
    type PrivateInput = ();

    fn prove<MT: MTConfig<Leaf = [F]>, S: CryptographicSponge>(
        namespace: NameSpace,
        _oracle_refs: &Self::RoundOracleRefs,
        _public_input: &Self::PublicInput,
        _private_input: &Self::PrivateInput,
        transcript: &mut Transcript<MT, S, F>,
        _prover_parameter: &Self::ProverParameter,
    ) -> Result<(), Error>
    where
        MT::InnerDigest: Absorb,
    {
        let mut rng = test_rng();

        // prover send
        let msg1 = (0..4).map(|_| F::rand(&mut rng));
        transcript.send_message(msg1);
        let msg2 = (0..256).map(|_| F::rand(&mut rng));
        transcript
            .send_message_oracle_with_localization(msg2, 2)
            .unwrap();
        let msg3 = (0..256).map(|_| F::rand(&mut rng));
        transcript
            .send_message_oracle_with_localization(msg3, 2)
            .unwrap();
        transcript
            .submit_prover_current_round(namespace, iop_trace!("mock send"))
            .unwrap();

        // verifier send
        let vm1 = transcript.squeeze_verifier_field_elements(&[
            FieldElementSize::Full,
            FieldElementSize::Full,
            FieldElementSize::Full,
        ]);
        let vm2 = transcript.squeeze_verifier_bytes(16);
        transcript.submit_verifier_current_round(namespace, iop_trace!("mock send"));

        // verifier send2
        transcript.squeeze_verifier_bits(19);
        transcript.submit_verifier_current_round(namespace, iop_trace!("mock send2"));

        // prover send
        let msg1 = vm1.into_iter().map(|x| x.square());
        transcript.send_message(msg1);
        let msg2 = (0..256u128).map(|x| {
            let rhs: F = vm2.to_field_elements().unwrap()[0];
            F::from(x) + rhs
        });
        transcript.send_message_oracle(msg2).unwrap();
        transcript
            .submit_prover_current_round(namespace, iop_trace!("mock send2"))
            .unwrap();

        // prover send 2
        let msg1 = (0..6).map(|_| F::rand(&mut rng));
        let msg2 = DensePolynomial::from_coefficients_vec(vec![
            F::from(0x12345u128),
            F::from(0x23456u128),
            F::from(0x34567u128),
            F::from(0x45678u128),
            F::from(0x56789u128),
        ]);
        transcript.send_message(msg1);
        transcript.send_univariate_polynomial(8, &msg2)?;
        transcript
            .submit_prover_current_round(namespace, iop_trace!("mock send3"))
            .unwrap();

        Ok(())
    }
}

pub(crate) struct MockTest1Verifier<F: PrimeField + Absorb> {
    _field: PhantomData<F>,
}

impl<S: CryptographicSponge, F: PrimeField + Absorb> IOPVerifier<S, F> for MockTest1Verifier<F> {
    type VerifierOutput = bool;
    type VerifierParameter = ();
    type OracleRefs = ();
    type PublicInput = ();

    fn register_iop_structure<MT: MTConfig<Leaf = [F]>>(
        namespace: NameSpace,
        transcript: &mut SimulationTranscript<MT, S, F>,
        _verifier_parameter: &Self::VerifierParameter,
    ) where
        MT::InnerDigest: Absorb,
    {
        // prover send
        let expected_info = ProverRoundMessageInfo {
            reed_solomon_code_degree_bound: vec![],
            num_message_oracles: 2,
            num_short_messages: 1,
            oracle_length: 256,
            localization_parameter: 2,
        };
        transcript.receive_prover_current_round(namespace, expected_info, iop_trace!());

        // verifier send
        transcript.squeeze_verifier_field_elements(&[
            FieldElementSize::Full,
            FieldElementSize::Full,
            FieldElementSize::Full,
        ]);
        transcript.squeeze_verifier_field_bytes(16);
        transcript.submit_verifier_current_round(namespace, iop_trace!());

        // verifier send2
        transcript.squeeze_verifier_field_bits(19);
        transcript.submit_verifier_current_round(namespace, iop_trace!());

        // prover send
        let expected_info = ProverRoundMessageInfo {
            reed_solomon_code_degree_bound: vec![],
            num_message_oracles: 1,
            num_short_messages: 1,
            oracle_length: 256,
            localization_parameter: 0,
        };
        transcript.receive_prover_current_round(namespace, expected_info, iop_trace!());

        // prover send2
        let expected_info = ProverRoundMessageInfo {
            reed_solomon_code_degree_bound: vec![8],
            num_message_oracles: 0,
            num_short_messages: 1,
            oracle_length: 128,
            localization_parameter: 0, // managed by LDT
        };
        transcript.receive_prover_current_round(namespace, expected_info, iop_trace!());
    }

    fn query_and_decide<O: RoundOracle<F>>(
        namespace: NameSpace,
        _verifier_parameter: &Self::VerifierParameter,
        _public_input: &Self::PublicInput,
        _verifier_state: &Self::OracleRefs,
        _sponge: &mut S,
        messages_in_commit_phase: &mut MessagesCollection<&mut O, VerifierMessage<F>>,
    ) -> Result<Self::VerifierOutput, Error> {
        // verify if message is indeed correct
        let mut rng = test_rng();
        let pm1_1: Vec<_> = (0..4).map(|_| F::rand(&mut rng)).collect();
        let pm1_2: Vec<_> = (0..256).map(|_| F::rand(&mut rng)).collect();
        let pm1_3: Vec<_> = (0..256).map(|_| F::rand(&mut rng)).collect();

        assert_eq!(
            messages_in_commit_phase
                .prover_message(namespace, 0)
                .get_short_message(0, iop_trace!()),
            &pm1_1
        );
        assert_eq!(
            messages_in_commit_phase
                .prover_message(namespace, 0)
                .query(&[123, 223], iop_trace!("mock query 0")),
            vec![vec![pm1_2[123], pm1_3[123]], vec![pm1_2[223], pm1_3[223]]]
        );

        let vm1_1 = if let VerifierMessage::FieldElements(fe) =
            messages_in_commit_phase.verifier_message(namespace, 0)[0].clone()
        {
            assert_eq!(fe.len(), 3);
            fe
        } else {
            panic!("invalid vm message type")
        };
        let vm1_2 = if let VerifierMessage::Bytes(bytes) =
            messages_in_commit_phase.verifier_message(namespace, 0)[1].clone()
        {
            assert_eq!(bytes.len(), 16);
            bytes
        } else {
            panic!("invalid vm message type");
        };

        if let VerifierMessage::Bits(bits) =
            &messages_in_commit_phase.verifier_message(namespace, 1)[0]
        {
            assert_eq!(bits.len(), 19);
        } else {
            panic!("invalid vm message type");
        }

        let pm2_1: Vec<_> = vm1_1.into_iter().map(|x| x.square()).collect();

        assert_eq!(
            messages_in_commit_phase
                .prover_message(namespace, 1)
                .get_short_message(0, iop_trace!()),
            &pm2_1
        );

        let pm2_2: Vec<_> = (0..256u128)
            .map(|x| {
                let rhs: F = vm1_2.to_field_elements().unwrap()[0];
                F::from(x) + rhs
            })
            .collect();

        assert_eq!(
            messages_in_commit_phase
                .prover_message(namespace, 1)
                .query(&[19, 29, 39], iop_trace!()),
            vec![vec![pm2_2[19]], vec![pm2_2[29]], vec![pm2_2[39]]]
        );

        let pm3_1: Vec<_> = (0..6).map(|_| F::rand(&mut rng)).collect();
        assert_eq!(
            messages_in_commit_phase
                .prover_message(namespace, 2)
                .get_short_message(0, iop_trace!()),
            &pm3_1
        );
        // just query some points
        messages_in_commit_phase
            .prover_message(namespace, 2)
            .query(&vec![1, 2], iop_trace!());

        Ok(true)
    }
}

#[test]
fn check_mock1_commit_phase() {
    let fri_parameters = FRIParameters::new(
        64,
        vec![1, 2, 1],
        Radix2CosetDomain::new_radix2_coset(128, Fr::one()),
    );
    let ldt_pamameters = LinearCombinationLDTParameters {
        fri_parameters,
        num_queries: 1,
    };
    let sponge = PoseidonSponge::new(&poseidon_parameters());
    check_commit_phase_correctness::<
        Fr,
        _,
        FieldMTConfig,
        MockTestProver<Fr>,
        MockTest1Verifier<Fr>,
        LinearCombinationLDT<Fr>,
    >(
        sponge,
        &(),
        &(),
        &(),
        &ldt_pamameters,
        MTHashParameters {
            leaf_hash_param: poseidon_parameters(),
            inner_hash_param: poseidon_parameters(),
        },
    );
}