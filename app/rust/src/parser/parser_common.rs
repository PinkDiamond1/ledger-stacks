#![allow(non_camel_case_types, non_upper_case_globals, non_snake_case)]
#![allow(clippy::upper_case_acronyms)]
use nom::{
    bytes::complete::take,
    number::complete::{be_u32, le_u8},
};

use crate::parser::{c32, error::ParserError};

// The max len for asset, contract and clarity names
pub const MAX_STRING_LEN: u8 = 128;
pub const HASH160_LEN: usize = 20;
pub const STACKS_ADDR_LEN: usize = HASH160_LEN + 1;

// The conversion constant between microSTX to STX
pub const STX_DECIMALS: u8 = 6;

pub const C32_ENCODED_ADDRS_LENGTH: usize = 48;

// The amount of post_conditions we can
// handle
pub const NUM_SUPPORTED_POST_CONDITIONS: usize = 16;
pub const SIGNATURE_LEN: usize = 65;
pub const MAX_STACKS_STRING_LEN: usize = 256;
pub const TOKEN_TRANSFER_MEMO_LEN: usize = 34;

/// Stacks transaction versions
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Copy)]
pub enum TransactionVersion {
    Mainnet = 0x00,
    Testnet = 0x80,
}

impl TransactionVersion {
    #[inline(never)]
    pub fn from_bytes(bytes: &[u8]) -> nom::IResult<&[u8], Self, ParserError> {
        let version_res = le_u8(bytes)?;
        let tx_version =
            Self::from_u8(version_res.1).ok_or(ParserError::parser_unexpected_error)?;
        Ok((version_res.0, tx_version))
    }

    #[inline(never)]
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(Self::Mainnet),
            0x80 => Some(Self::Testnet),
            _ => None,
        }
    }
}

#[repr(u8)]
#[derive(Clone, PartialEq, Copy, Debug)]
pub enum AssetInfoId {
    STX = 0,
    FungibleAsset = 1,
    NonfungibleAsset = 2,
}

impl AssetInfoId {
    #[inline(never)]
    pub fn from_u8(b: u8) -> Option<AssetInfoId> {
        match b {
            0 => Some(AssetInfoId::STX),
            1 => Some(AssetInfoId::FungibleAsset),
            2 => Some(AssetInfoId::NonfungibleAsset),
            _ => None,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AssetInfo<'a> {
    pub address: StacksAddress<'a>,
    pub contract_name: ContractName<'a>,
    pub asset_name: ClarityName<'a>,
}

impl<'a> AssetInfo<'a> {
    #[inline(never)]
    pub fn from_bytes(bytes: &'a [u8]) -> nom::IResult<&[u8], Self, ParserError> {
        let address = StacksAddress::from_bytes(bytes)?;
        let (raw, contract_name) = ContractName::from_bytes(address.0)?;
        let asset_name = ClarityName::from_bytes(raw)?;
        Ok((
            asset_name.0,
            Self {
                address: address.1,
                contract_name,
                asset_name: asset_name.1,
            },
        ))
    }

    #[inline(never)]
    pub fn read_as_bytes(bytes: &'a [u8]) -> nom::IResult<&[u8], &[u8], ParserError> {
        let (raw, _) = StacksAddress::from_bytes(bytes)?;
        let (raw1, _) = ContractName::from_bytes(raw)?;
        let (raw2, _) = ClarityName::from_bytes(raw1)?;
        let len = bytes.len() - raw2.len();
        let (left, inner) = take(len)(bytes)?;
        Ok((left, inner))
    }

    pub fn asset_name(&self) -> &[u8] {
        self.asset_name.0
    }
}

#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Copy)]
// Flag used to know if the signer is valid and
// who is
pub enum SignerId {
    Origin,
    Sponsor,
    Invalid,
}

// tag address hash modes as "singlesig" or "multisig" so we can't accidentally construct an
// invalid spending condition
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HashMode {
    P2PKH = 0x00,
    P2SH = 0x01,
    P2WPKH = 0x02,
    P2WSH = 0x03,
}

impl HashMode {
    #[inline(never)]
    pub fn from_u8(n: u8) -> Option<HashMode> {
        match n {
            x if x == HashMode::P2PKH as u8 => Some(HashMode::P2PKH),
            x if x == HashMode::P2WPKH as u8 => Some(HashMode::P2WPKH),
            x if x == HashMode::P2SH as u8 => Some(HashMode::P2SH),
            x if x == HashMode::P2WSH as u8 => Some(HashMode::P2WSH),
            _ => None,
        }
    }

    pub fn to_version_mainnet(self) -> u8 {
        match self {
            HashMode::P2PKH => c32::C32_ADDRESS_VERSION_MAINNET_SINGLESIG,
            _ => c32::C32_ADDRESS_VERSION_MAINNET_MULTISIG,
        }
    }

    pub fn to_version_testnet(self) -> u8 {
        match self {
            HashMode::P2PKH => c32::C32_ADDRESS_VERSION_TESTNET_SINGLESIG,
            _ => c32::C32_ADDRESS_VERSION_TESTNET_MULTISIG,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StacksString<'a>(&'a [u8]);

impl<'a> StacksString<'a> {
    #[inline(never)]
    pub fn from_bytes(bytes: &'a [u8]) -> nom::IResult<&[u8], Self, ParserError> {
        let len = be_u32(bytes)?;
        let string_len = len.1 as usize;
        if string_len > MAX_STACKS_STRING_LEN && !crate::is_expert_mode() {
            return Err(nom::Err::Error(ParserError::parser_stacks_string_too_long));
        }
        let string = take(string_len)(len.0)?;
        Ok((string.0, Self(string.1)))
    }
}

// contract name with valid charactes being
// ^[a-zA-Z]([a-zA-Z0-9]|[-_])*$
#[repr(C)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ContractName<'a>(pub &'a [u8]);

impl<'a> ContractName<'a> {
    #[inline(never)]
    pub fn from_bytes(bytes: &'a [u8]) -> nom::IResult<&[u8], Self, ParserError> {
        let len = u8_with_limits(MAX_STRING_LEN, bytes)
            .map_err(|_| ParserError::parser_invalid_contract_name)?;
        let nameLen = len.1;
        let name = take(nameLen as usize)(len.0)?;
        // TODO: Verify if the name has valid characters
        Ok((name.0, Self(name.1)))
    }
}

// Represent a clarity contract name with valid characters being
// ^[a-zA-Z]([a-zA-Z0-9]|[-_!?+<>=/*])*$|^[-+=/*]$|^[<>]=?$
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClarityName<'a>(pub &'a [u8]);

impl<'a> ClarityName<'a> {
    #[inline(never)]
    pub fn from_bytes(bytes: &'a [u8]) -> nom::IResult<&[u8], Self, ParserError> {
        let len = u8_with_limits(MAX_STRING_LEN, bytes)
            .map_err(|_| ParserError::parser_invalid_asset_name)?;
        let name_len = len.1;
        let name = take(name_len as usize)(len.0)?;
        // TODO: Verify if the name has valid characters
        Ok((name.0, Self(name.1)))
    }

    #[inline(never)]
    pub fn read_as_bytes(bytes: &'a [u8]) -> nom::IResult<&[u8], &[u8], ParserError> {
        let len = u8_with_limits(MAX_STRING_LEN, bytes)
            .map_err(|_| ParserError::parser_invalid_asset_name)?;
        let name_len = len.1 as usize;
        let (raw, name_bytes) = take(name_len + 1)(len.0)?;
        // TODO: Verify if the name has valid characters
        Ok((raw, name_bytes))
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
// we take HASH160_LEN + 1-byte hash mode
pub struct StacksAddress<'a>(pub &'a [u8; STACKS_ADDR_LEN]);

impl<'a> StacksAddress<'a> {
    #[inline(never)]
    pub fn from_bytes(bytes: &'a [u8]) -> nom::IResult<&[u8], Self, ParserError> {
        let (raw, _) = take(STACKS_ADDR_LEN)(bytes)?;
        let address = arrayref::array_ref!(bytes, 0, STACKS_ADDR_LEN);
        Ok((raw, Self(address)))
    }

    pub fn encoded_address(
        &self,
    ) -> Result<arrayvec::ArrayVec<[u8; C32_ENCODED_ADDRS_LENGTH]>, ParserError> {
        c32::c32_address(self.0[0], &self.0[1..])
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct StandardPrincipal<'a>(pub &'a [u8]);

impl<'a> StandardPrincipal<'a> {
    #[inline(never)]
    pub fn from_bytes(bytes: &'a [u8]) -> nom::IResult<&[u8], Self, ParserError> {
        let (raw, address) = take(HASH160_LEN + 1usize)(bytes)?;
        Ok((raw, Self(address)))
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ContractPrincipal<'a>(StandardPrincipal<'a>, ContractName<'a>);
impl<'a> ContractPrincipal<'a> {
    #[inline(never)]
    pub fn from_bytes(bytes: &'a [u8]) -> nom::IResult<&[u8], Self, ParserError> {
        let address = StandardPrincipal::from_bytes(bytes)?;
        let name = ContractName::from_bytes(address.0)?;
        Ok((name.0, Self(address.1, name.1)))
    }

    pub fn read_as_bytes(bytes: &'a [u8]) -> nom::IResult<&[u8], &[u8], ParserError> {
        let (rem, _) = Self::from_bytes(bytes)?;
        let len = bytes.len() - rem.len();
        let (rem, self_bytes) = take(len)(bytes)?;
        Ok((rem, self_bytes))
    }
}

#[repr(C)]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PrincipalData<'a> {
    pub data: (StandardPrincipal<'a>, Option<ContractName<'a>>),
}

impl<'a> PrincipalData<'a> {
    #[inline(never)]
    pub fn standard_from_bytes(bytes: &'a [u8]) -> nom::IResult<&[u8], Self, ParserError> {
        let (raw, principal) = StandardPrincipal::from_bytes(bytes)?;
        Ok((
            raw,
            Self {
                data: (principal, None),
            },
        ))
    }

    #[inline(never)]
    pub fn contract_principal_from_bytes(
        bytes: &'a [u8],
    ) -> nom::IResult<&[u8], Self, ParserError> {
        let (raw, address) = StandardPrincipal::from_bytes(bytes)?;
        let (raw2, name) = ContractName::from_bytes(raw)?;
        Ok((
            raw2,
            Self {
                data: (address, Some(name)),
            },
        ))
    }

    pub fn version(&self) -> u8 {
        (self.data.0).0[0]
    }

    pub fn raw_address(&self) -> &[u8] {
        &(self.data.0).0[1..]
    }

    #[inline(never)]
    pub fn encoded_address(
        &self,
    ) -> Result<arrayvec::ArrayVec<[u8; C32_ENCODED_ADDRS_LENGTH]>, ParserError> {
        let version = self.version();
        let address = self.raw_address();
        c32::c32_address(version, address)
    }
}

/******************************* NOM parser combinators *******************************************/

pub fn u8_with_limits(limit: u8, bytes: &[u8]) -> nom::IResult<&[u8], u8, ParserError> {
    if !bytes.is_empty() && bytes[0] <= limit {
        le_u8(bytes)
    } else {
        Err(nom::Err::Error(ParserError::parser_value_out_of_range))
    }
}
