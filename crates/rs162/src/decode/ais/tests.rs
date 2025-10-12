//! Integration tests for AIS message dispatching
//!
//! This module tests that the main Message enum correctly dispatches
//! to the appropriate message types based on the message type field.

use super::*;

fn decode_message(sentence: &str) -> Message {
    let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
    let binary_data = nmea_msg.payload_to_binary().unwrap();

    let cursor = std::io::Cursor::new(binary_data);
    let mut reader = Reader::new(cursor);
    Message::from_reader_with_ctx(&mut reader, ()).unwrap()
}
fn decode_message2(sentence1: &str, sentence2: &str) -> Message {
    let nmea_msg1 = NmeaAisMessage::parse(sentence1).unwrap();
    let nmea_msg2 = NmeaAisMessage::parse(sentence2).unwrap();
    let messages = vec![nmea_msg1, nmea_msg2];
    let binary_data = MessageAssembler::assemble_from_iterable(messages).unwrap();

    let cursor = std::io::Cursor::new(binary_data);
    let mut reader = Reader::new(cursor);
    Message::from_reader_with_ctx(&mut reader, ()).unwrap()
}

#[test]
fn test_type_1_dispatch() {
    let msg = decode_message("!AIVDM,1,1,,B,15M67FC000G?ufbE`FepT@3n00Sa,0*5C");

    if let Message::PositionReport1(position_report) = msg {
        assert_eq!(position_report.msg_type, 1);
        assert_eq!(position_report.mmsi, 366053209);
    } else {
        panic!("Expected PositionReport1, got {:?}", msg);
    }
}

#[test]
fn test_type_3_dispatch() {
    let msg = decode_message("!AIVDM,1,1,,A,35NSH95001G?wopE`beasVk@0E5:,0*6F");

    if let Message::PositionReport3(position_report) = msg {
        assert_eq!(position_report.msg_type, 3);
        assert_eq!(position_report.mmsi, 367581220);
    } else {
        panic!("Expected PositionReport3, got {:?}", msg);
    }
}

#[test]
fn test_type_4_dispatch() {
    let msg = decode_message("!AIVDM,1,1,,B,403OtVAv>lba;o?Ia`E`4G?02H6k,0*44");

    if let Message::BaseStationTimeReport(base_station) = msg {
        assert_eq!(base_station.msg_type, 4);
        assert_eq!(base_station.mmsi, 3669145);
    } else {
        panic!("Expected BaseStationTimeReport, got {:?}", msg);
    }
}

#[test]
fn test_type_5_dispatch() {
    let msg = decode_message2(
        "!AIVDM,2,1,1,A,55?MbV02;H;s<HtKR20EHE:0@T4@Dn2222222216L961O5Gf0NSQEp6ClRp8,0*1C",
        "!AIVDM,2,2,1,A,88888888880,2*25",
    );

    if let Message::StaticAndVoyageData(static_data) = msg {
        assert_eq!(static_data.msg_type, 5);
    } else {
        panic!("Expected StaticAndVoyageData, got {:?}", msg);
    }
}

#[test]
fn test_type_6_dispatch() {
    let msg = decode_message("!AIVDM,1,1,,B,6B?n;be:cbapalgc;i6?Ow4,2*4A");

    if let Message::BinaryAddressedMessage(binary_msg) = msg {
        assert_eq!(binary_msg.msg_type, 6);
        assert_eq!(binary_msg.mmsi, 150834090);
    } else {
        panic!("Expected BinaryAddressedMessage, got {:?}", msg);
    }
}

#[test]
fn test_type_7_dispatch() {
    let msg = decode_message("!AIVDM,1,1,,A,702R5`hwCjq8,0*6B");

    if let Message::BinaryAcknowledge(ack_msg) = msg {
        assert_eq!(ack_msg.msg_type, 7);
        assert_eq!(ack_msg.mmsi, 2655651);
    } else {
        panic!("Expected BinaryAcknowledge, got {:?}", msg);
    }
}

#[test]
fn test_type_8_dispatch() {
    let msg =
        decode_message("!AIVDM,1,1,,A,85Mwp`1Kf3aCnsNvBWLi=wQuNhA5t43N`5nCuI=p<IBfVqnMgPGs,0*47");

    if let Message::BinaryBroadcastMessage(binary_msg) = msg {
        if let BinaryBroadcastMessage::Default(default_msg) = binary_msg {
            assert_eq!(default_msg.msg_type, 8);
            assert_eq!(default_msg.mmsi, 366999712);
        } else {
            panic!("Expected Default variant of BinaryBroadcastMessage");
        }
    } else {
        panic!("Expected BinaryBroadcastMessage, got {:?}", msg);
    }
}

#[test]
fn test_type_9_dispatch() {
    let msg = decode_message("!AIVDM,1,1,,B,91b55wi;hbOS@OdQAC062Ch2089h,0*30");

    if let Message::SarAircraftPositionReport(sar_msg) = msg {
        assert_eq!(sar_msg.msg_type, 9);
        assert_eq!(sar_msg.mmsi, 111232511);
    } else {
        panic!("Expected SarAircraftPositionReport, got {:?}", msg);
    }
}

#[test]
fn test_type_10_dispatch() {
    let msg = decode_message("!AIVDM,1,1,,B,:5MlU41GMK6@,0*6C");

    if let Message::UtcDateInquiry(utc_msg) = msg {
        assert_eq!(utc_msg.msg_type, 10);
    } else {
        panic!("Expected UtcDateInquiry, got {:?}", msg);
    }
}

#[test]
fn test_type_11_dispatch() {
    let msg = decode_message("!AIVDM,1,1,,B,;4R33:1uUK2F`q?mOt@@GoQ00000,0*5D");

    if let Message::BaseStationTimeReport11(base_station) = msg {
        assert_eq!(base_station.msg_type, 11);
    } else {
        panic!("Expected BaseStationTimeReport11, got {:?}", msg);
    }
}

#[test]
fn test_type_12_dispatch() {
    let msg = decode_message("!AIVDM,1,1,,A,<5?SIj1;GbD07??4,0*38");

    if let Message::AddressedSafetyMessage(safety_msg) = msg {
        assert_eq!(safety_msg.msg_type, 12);
        assert_eq!(safety_msg.mmsi, 351853000);
    } else {
        panic!("Expected AddressedSafetyMessage, got {:?}", msg);
    }
}

#[test]
fn test_type_14_dispatch() {
    let msg = decode_message("!AIVDM,1,1,,A,>5?Per18=HB1U:1@E=B0m<L,2*51");

    if let Message::SafetyBroadcastMessage(safety_msg) = msg {
        assert_eq!(safety_msg.msg_type, 14);
        assert_eq!(safety_msg.mmsi, 351809000);
    } else {
        panic!("Expected SafetyBroadcastMessage, got {:?}", msg);
    }
}

#[test]
fn test_type_15_dispatch() {
    let msg = decode_message("!AIVDM,1,1,,A,?5OP=l00052HD00,2*5B");

    if let Message::Interrogation(interrogation_msg) = msg {
        assert_eq!(interrogation_msg.msg_type, 15);
        assert_eq!(interrogation_msg.mmsi, 368578000);
    } else {
        panic!("Expected Interrogation, got {:?}", msg);
    }
}

#[test]
fn test_type_16_dispatch() {
    let msg = decode_message("!AIVDM,1,1,,A,@01uEO@mMk7P<P00,0*18");

    if let Message::AssignmentModeCommand(assignment_msg) = msg {
        if let AssignmentModeCommand::Single(single_msg) = assignment_msg {
            assert_eq!(single_msg.msg_type, 16);
            assert_eq!(single_msg.mmsi, 2053501);
        } else {
            panic!("Expected Single variant of AssignmentModeCommand");
        }
    } else {
        panic!("Expected AssignmentModeCommand, got {:?}", msg);
    }
}

#[test]
fn test_type_17_dispatch() {
    let msg = decode_message2(
        "!AIVDM,2,1,5,A,A02VqLPA4I6C07h5Ed1h<OrsuBTTwS?r:C?w`?la<gno1RTRwSP9:BcurA8a,0*3A",
        "!AIVDM,2,2,5,A,:Oko02TSwu8<:Jbb,0*11",
    );

    if let Message::DgnssBroadcastMessage(dgnss_msg) = msg {
        assert_eq!(dgnss_msg.msg_type, 17);
        assert_eq!(dgnss_msg.mmsi, 2734450);
    } else {
        panic!("Expected DgnssBroadcastMessage, got {:?}", msg);
    }
}

#[test]
fn test_type_18_dispatch() {
    let msg = decode_message("!AIVDO,1,1,,A,B5NJ;PP2aUl4ot5Isbl6GwsUkP06,0*35");

    if let Message::ClassBPositionReport(class_b_msg) = msg {
        assert_eq!(class_b_msg.msg_type, 18);
    } else {
        panic!("Expected ClassBPositionReport, got {:?}", msg);
    }
}

#[test]
fn test_type_19_dispatch() {
    let msg =
        decode_message("!AIVDM,1,1,,B,C5N3SRgPEnJGEBT>NhWAwwo862PaLELTBJ:V00000000S0D:R220,0*0B");

    if let Message::ExtendedClassBPositionReport(extended_msg) = msg {
        assert_eq!(extended_msg.msg_type, 19);
    } else {
        panic!("Expected ExtendedClassBPositionReport, got {:?}", msg);
    }
}

#[test]
fn test_type_20_dispatch() {
    let msg = decode_message("!AIVDM,1,1,,A,D028rqP<QNfp000000000000000,2*0C");

    if let Message::DataLinkManagementMessage(data_link_msg) = msg {
        assert_eq!(data_link_msg.msg_type, 20);
        assert_eq!(data_link_msg.mmsi, 2243302);
    } else {
        panic!("Expected DataLinkManagementMessage, got {:?}", msg);
    }
}

#[test]
fn test_type_21_dispatch() {
    let msg = decode_message2(
        "!AIVDM,2,1,7,B,E4eHJhPR37q0000000000000000KUOSc=rq4h00000a,0*4A",
        "!AIVDM,2,2,7,B,@20,4*54",
    );

    if let Message::AidToNavigationReport(aid_msg) = msg {
        assert_eq!(aid_msg.msg_type, 21);
        assert_eq!(aid_msg.mmsi, 316021442);
    } else {
        panic!("Expected AidToNavigationReport, got {:?}", msg);
    }
}

#[test]
fn test_type_22_broadcast_dispatch() {
    let msg = decode_message("!AIVDM,1,1,,B,F030p:j2N2P5aJR0r;6f3rj10000,0*11");

    if let Message::ChannelManagement(channel_msg) = msg {
        if let ChannelManagement::Broadcast(broadcast_msg) = channel_msg {
            assert_eq!(broadcast_msg.msg_type, 22);
            assert_eq!(broadcast_msg.mmsi, 3160107);
        } else {
            panic!("Expected Broadcast variant of ChannelManagement");
        }
    } else {
        panic!("Expected ChannelManagement, got {:?}", msg);
    }
}

#[test]
fn test_type_23_dispatch() {
    let msg = decode_message("!AIVDM,1,1,,B,G02:Kn01R`sn@291nj600000900,2*12");

    if let Message::GroupAssignmentCommand(group_msg) = msg {
        assert_eq!(group_msg.msg_type, 23);
        assert_eq!(group_msg.mmsi, 2268120);
    } else {
        panic!("Expected GroupAssignmentCommand, got {:?}", msg);
    }
}

#[test]
fn test_type_24_part_b_dispatch() {
    let msg = decode_message("!AIVDM,1,1,,A,H52KMeDU653hhhi0000000000000,0*1A");

    if let Message::StaticDataReport(static_msg) = msg {
        if let StaticDataReport::PartB(part_b_msg) = static_msg {
            assert_eq!(part_b_msg.msg_type, 24);
            assert_eq!(part_b_msg.mmsi, 338091445);
            assert_eq!(part_b_msg.partno, 1);
        } else {
            panic!("Expected PartB variant of StaticDataReport");
        }
    } else {
        panic!("Expected StaticDataReport, got {:?}", msg);
    }
}

#[test]
fn test_type_24_part_a_dispatch() {
    let msg = decode_message("!AIVDO,1,1,,A,H1mg=5@EP4m0hF1<PU000000000,2*77");

    if let Message::StaticDataReport(static_msg) = msg {
        if let StaticDataReport::PartA(part_a_msg) = static_msg {
            assert_eq!(part_a_msg.msg_type, 24);
            assert_eq!(part_a_msg.mmsi, 123456789);
            assert_eq!(part_a_msg.partno, 0);
        } else {
            panic!("Expected PartA variant of StaticDataReport");
        }
    } else {
        panic!("Expected StaticDataReport, got {:?}", msg);
    }
}

#[test]
fn test_type_27_dispatch() {
    let msg = decode_message("!AIVDO,1,1,,A,K01;FQh?PbtE3P00,0*75");

    if let Message::LongRangeAisBroadcastMessage(long_range_msg) = msg {
        assert_eq!(long_range_msg.msg_type, 27);
        assert_eq!(long_range_msg.mmsi, 1234567);
    } else {
        panic!("Expected LongRangeAisBroadcastMessage, got {:?}", msg);
    }
}
