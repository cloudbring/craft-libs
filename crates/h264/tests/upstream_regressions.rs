use craft_bitstream::BitWriter;
use craft_h264::{
    Error,
    dpb::Dpb,
    params::{Pps, Sps},
    slice::{NalHeader, PocState, RefPicListMod, SliceHeader},
};

fn sps_pps() -> (Sps, Pps) {
    let mut writer = BitWriter::new();
    writer.write_bits(66, 8);
    writer.write_bits(0xc0, 8);
    writer.write_bits(30, 8);
    writer.write_ue(0);
    writer.write_ue(0);
    writer.write_ue(2);
    writer.write_ue(1);
    writer.write_bit(false);
    writer.write_ue(10);
    writer.write_ue(8);
    writer.write_bit(true);
    writer.write_bit(true);
    writer.write_bit(false);
    writer.write_bit(false);
    writer.rbsp_trailing();
    let sps = Sps::parse(&writer.finish()).unwrap();
    let mut writer = BitWriter::new();
    writer.write_ue(0);
    writer.write_ue(0);
    writer.write_bit(false);
    writer.write_bit(false);
    writer.write_ue(0);
    writer.write_ue(0);
    writer.write_ue(0);
    writer.write_bit(false);
    writer.write_bits(0, 2);
    writer.write_se(0);
    writer.write_se(0);
    writer.write_se(0);
    writer.write_bit(false);
    writer.write_bit(false);
    writer.write_bit(false);
    writer.rbsp_trailing();
    let pps = Pps::parse(&writer.finish(), &[Some(sps.clone())]).unwrap();
    (sps, pps)
}

fn parse_override(qp_delta: i32) -> craft_h264::Result<SliceHeader> {
    let (sps, pps) = sps_pps();
    let mut writer = BitWriter::new();
    writer.write_ue(0);
    writer.write_ue(0);
    writer.write_ue(0);
    writer.write_bits(0, 4);
    writer.write_bit(true);
    writer.write_ue(0);
    writer.write_bit(false);
    writer.write_bit(false);
    writer.write_se(qp_delta);
    writer.rbsp_trailing();
    let nal = NalHeader { nal_ref_idc: 1, nal_unit_type: 1 };
    SliceHeader::parse(&writer.finish(), nal, |_| Ok((&pps, &sps))).map(|(header, _, _)| header)
}

#[test]
fn huge_slice_qp_delta_is_invalid() {
    assert!(matches!(parse_override(i32::MAX), Err(Error::Invalid(_))));
}

#[test]
fn poc0_huge_bottom_delta_does_not_overflow() {
    let (mut sps, _) = sps_pps();
    sps.pic_order_cnt_type = 0;
    sps.log2_max_poc_lsb = 4;
    let mut header = parse_override(0).unwrap();
    header.pic_order_cnt_lsb = 1;
    header.delta_pic_order_cnt_bottom = i32::MAX;
    let poc = PocState::default().compute(&header, &sps);
    assert_eq!(poc.top, 1);
    assert_eq!(poc.bottom, i32::MAX);
}

#[test]
fn ref_list_mod_huge_value_does_not_panic() {
    let mut header = parse_override(0).unwrap();
    for idc in [0, 1] {
        for value in [i32::MAX as u32, u32::MAX] {
            header.ref_pic_list_mod[0] = vec![RefPicListMod { idc, value }];
            assert!(Dpb::<u32>::new().build_ref_lists(&header, 0, 16, |entry| entry.frame).is_err());
        }
    }
}

#[test]
fn poc_extreme_offsets_do_not_overflow() {
    let (mut sps, _) = sps_pps();
    sps.log2_max_frame_num = 16;
    sps.offset_for_ref_frame = vec![i32::MAX, i32::MAX];
    sps.offset_for_non_ref_pic = i32::MAX;
    sps.offset_for_top_to_bottom_field = i32::MAX;
    let mut state = PocState::default();
    for _ in 0..32767 {
        state.update_gap_frame(5, &sps);
        state.update_gap_frame(1, &sps);
    }
    state.update_gap_frame(5, &sps);
    let mut header = parse_override(0).unwrap();
    header.frame_num = 1;
    header.delta_pic_order_cnt = [i32::MAX; 2];
    for poc_type in [1, 2] {
        sps.pic_order_cnt_type = poc_type;
        for ref_idc in [0, 1] {
            header.nal_ref_idc = ref_idc;
            let _ = state.compute(&header, &sps);
        }
    }
}
