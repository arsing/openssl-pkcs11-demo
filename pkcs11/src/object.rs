/// A reference to an object stored in a slot.
pub struct Object<T> {
	session: std::sync::Arc<crate::Session>,
	handle: pkcs11_sys::CK_OBJECT_HANDLE,
	_key: std::marker::PhantomData<T>,
}

impl<T> Object<T> {
	pub(crate) fn new(
		session: std::sync::Arc<crate::Session>,
		handle: pkcs11_sys::CK_OBJECT_HANDLE,
	) -> Self {
		Object {
			session,
			handle,
			_key: Default::default(),
		}
	}
}

impl Object<openssl::ec::EcKey<openssl::pkey::Public>> {
	/// Get the EC parameters of this EC public key object.
	pub fn parameters(&self) -> Result<openssl::ec::EcKey<openssl::pkey::Public>, GetKeyParametersError> {
		unsafe {
			let curve = get_attribute_value_byte_buf(
				&self.session,
				self,
				pkcs11_sys::CKA_EC_PARAMS,
				self.session.context.C_GetAttributeValue,
			)?;
			let curve = super::EcCurve::from_oid_der(&curve).ok_or_else(|| GetKeyParametersError::UnrecognizedEcCurve(curve))?;
			let curve = curve.as_nid();
			let group = openssl::ec::EcGroup::from_curve_name(curve).map_err(GetKeyParametersError::ConvertToOpenssl)?;

			// CKA_EC_POINT returns a DER encoded octet string representing the point.
			//
			// The octet string is in the RFC 5480 format which is exactly what EC_POINT_oct2point expected, so we just need to strip the DER type and length prefix.
			let point = get_attribute_value_byte_buf(
				&self.session,
				self,
				pkcs11_sys::CKA_EC_POINT,
				self.session.context.C_GetAttributeValue,
			)?;
			let point =
				openssl_sys2::d2i_ASN1_OCTET_STRING(
					std::ptr::null_mut(),
					&mut (point.as_ptr() as _),
					point.len() as _,
				);
			if point.is_null() {
				return Err(GetKeyParametersError::MalformedEcPoint(openssl::error::ErrorStack::get()));
			}
			let point: openssl::asn1::Asn1String = foreign_types::ForeignType::from_ptr(point);
			let mut big_num_context = openssl::bn::BigNumContext::new().map_err(GetKeyParametersError::ConvertToOpenssl)?;
			let point = openssl::ec::EcPoint::from_bytes(&group, point.as_slice(), &mut big_num_context).map_err(GetKeyParametersError::ConvertToOpenssl)?;

			let parameters = openssl::ec::EcKey::<openssl::pkey::Public>::from_public_key(
				&group,
				&point,
			).map_err(GetKeyParametersError::ConvertToOpenssl)?;
			Ok(parameters)
		}
	}
}

/// An error from getting the parameters of a key object.
#[derive(Debug)]
pub enum GetKeyParametersError {
	ConvertToOpenssl(openssl::error::ErrorStack),
	GetAttributeValueFailed(pkcs11_sys::CK_RV),
	MalformedEcPoint(openssl::error::ErrorStack),
	UnrecognizedEcCurve(Vec<u8>),
}

impl std::fmt::Display for GetKeyParametersError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			GetKeyParametersError::ConvertToOpenssl(_) => write!(f, "could not convert components to openssl types"),
			GetKeyParametersError::GetAttributeValueFailed(result) => write!(f, "C_GetAttributeValue failed with {}", result),
			GetKeyParametersError::MalformedEcPoint(_) => write!(f, "could not parse the DER-encoded EC point"),
			GetKeyParametersError::UnrecognizedEcCurve(curve) => write!(f, "the EC point is using an unknown curve: {:?}", curve),
		}
	}
}

impl Object<openssl::rsa::Rsa<openssl::pkey::Public>> {
	/// Get the RSA parameters of this RSA public key object.
	pub fn parameters(&self) -> Result<openssl::rsa::Rsa<openssl::pkey::Public>, GetKeyParametersError> {
		unsafe {
			let modulus = get_attribute_value_byte_buf(
				&self.session,
				self,
				pkcs11_sys::CKA_MODULUS,
				self.session.context.C_GetAttributeValue,
			)?;
			let modulus = openssl::bn::BigNum::from_slice(&modulus).map_err(GetKeyParametersError::ConvertToOpenssl)?;

			let public_exponent = get_attribute_value_byte_buf(
				&self.session,
				self,
				pkcs11_sys::CKA_PUBLIC_EXPONENT,
				self.session.context.C_GetAttributeValue,
			)?;
			let public_exponent = openssl::bn::BigNum::from_slice(&public_exponent).map_err(GetKeyParametersError::ConvertToOpenssl)?;

			let parameters = openssl::rsa::Rsa::<openssl::pkey::Public>::from_public_components(
				modulus,
				public_exponent,
			).map_err(GetKeyParametersError::ConvertToOpenssl)?;
			Ok(parameters)
		}
	}
}

impl std::error::Error for GetKeyParametersError {
	#[allow(clippy::match_same_arms)]
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			GetKeyParametersError::ConvertToOpenssl(inner) => Some(inner),
			GetKeyParametersError::GetAttributeValueFailed(_) => None,
			GetKeyParametersError::MalformedEcPoint(inner) => Some(inner),
			GetKeyParametersError::UnrecognizedEcCurve(_) => None,
		}
	}
}

impl Object<openssl::ec::EcKey<openssl::pkey::Private>> {
	pub fn sign(&self, digest: &[u8], signature: &mut [u8]) -> Result<pkcs11_sys::CK_ULONG, SignError> {
		unsafe {
			// Signing with the private key needs login
			self.session.login().map_err(SignError::LoginFailed)?;

			let mechanism = pkcs11_sys::CK_MECHANISM_IN {
				mechanism: pkcs11_sys::CKM_ECDSA,
				pParameter: std::ptr::null(),
				ulParameterLen: 0,
			};
			let result =
				(self.session.context.C_SignInit)(
					self.session.handle,
					&mechanism,
					self.handle,
				);
			if result != pkcs11_sys::CKR_OK {
				return Err(SignError::SignInitFailed(result));
			}

			let original_signature_len = signature.len() as _;
			let mut signature_len = original_signature_len;

			let result =
				(self.session.context.C_Sign)(
					self.session.handle,
					digest.as_ptr(),
					digest.len() as _,
					signature.as_mut_ptr(),
					&mut signature_len,
				);
			if result != pkcs11_sys::CKR_OK {
				return Err(SignError::SignFailed(result));
			}
			assert!(signature_len <= original_signature_len);

			Ok(signature_len)
		}
	}
}

impl Object<openssl::rsa::Rsa<openssl::pkey::Private>> {
	pub fn sign(&self, mechanism: pkcs11_sys::CK_MECHANISM_TYPE, digest: &[u8], signature: &mut [u8]) -> Result<pkcs11_sys::CK_ULONG, SignError> {
		unsafe {
			// Signing with the private key needs login
			self.session.login().map_err(SignError::LoginFailed)?;

			let mechanism = pkcs11_sys::CK_MECHANISM_IN {
				mechanism,
				pParameter: std::ptr::null(),
				ulParameterLen: 0,
			};
			let result =
				(self.session.context.C_SignInit)(
					self.session.handle,
					&mechanism,
					self.handle,
				);
			if result != pkcs11_sys::CKR_OK {
				return Err(SignError::SignInitFailed(result));
			}

			let original_signature_len = signature.len() as _;
			let mut signature_len = original_signature_len;

			let result =
				(self.session.context.C_Sign)(
					self.session.handle,
					digest.as_ptr(),
					digest.len() as _,
					signature.as_mut_ptr(),
					&mut signature_len,
				);
			if result != pkcs11_sys::CKR_OK {
				return Err(SignError::SignFailed(result));
			}
			assert!(signature_len <= original_signature_len);

			Ok(signature_len)
		}
	}
}

#[derive(Debug)]
#[allow(clippy::pub_enum_variant_names)]
pub enum SignError {
	LoginFailed(crate::LoginError),
	SignInitFailed(pkcs11_sys::CK_RV),
	SignFailed(pkcs11_sys::CK_RV),
}

impl std::fmt::Display for SignError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			SignError::LoginFailed(_) => f.write_str("could not log in to the token"),
			SignError::SignInitFailed(result) => write!(f, "C_SignInit failed with {}", result),
			SignError::SignFailed(result) => write!(f, "C_Sign failed with {}", result),
		}
	}
}

impl std::error::Error for SignError {
	#[allow(clippy::match_same_arms)]
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			SignError::LoginFailed(inner) => Some(inner),
			SignError::SignInitFailed(_) => None,
			SignError::SignFailed(_) => None,
		}
	}
}

impl Object<openssl::rsa::Rsa<openssl::pkey::Public>> {
	pub fn encrypt(&self, mechanism: pkcs11_sys::CK_MECHANISM_TYPE, plaintext: &[u8], ciphertext: &mut [u8]) -> Result<pkcs11_sys::CK_ULONG, EncryptError> {
		unsafe {
			let mechanism = pkcs11_sys::CK_MECHANISM_IN {
				mechanism,
				pParameter: std::ptr::null(),
				ulParameterLen: 0,
			};
			let result =
				(self.session.context.C_EncryptInit)(
					self.session.handle,
					&mechanism,
					self.handle,
				);
			if result != pkcs11_sys::CKR_OK {
				return Err(EncryptError::EncryptInitFailed(result));
			}

			let original_ciphertext_len = ciphertext.len() as _;
			let mut ciphertext_len = original_ciphertext_len;

			let result =
				(self.session.context.C_Encrypt)(
					self.session.handle,
					plaintext.as_ptr(),
					plaintext.len() as _,
					ciphertext.as_mut_ptr(),
					&mut ciphertext_len,
				);
			if result != pkcs11_sys::CKR_OK {
				return Err(EncryptError::EncryptFailed(result));
			}
			assert!(ciphertext_len <= original_ciphertext_len);

			Ok(ciphertext_len)
		}
	}
}

#[derive(Debug)]
pub enum EncryptError {
	EncryptInitFailed(pkcs11_sys::CK_RV),
	EncryptFailed(pkcs11_sys::CK_RV),
}

impl std::fmt::Display for EncryptError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			EncryptError::EncryptInitFailed(result) => write!(f, "C_EncryptInit failed with {}", result),
			EncryptError::EncryptFailed(result) => write!(f, "C_Encrypt failed with {}", result),
		}
	}
}

impl std::error::Error for EncryptError {
}

/// Query an attribute value as a byte buffer of arbitrary length.
unsafe fn get_attribute_value_byte_buf<T>(
	session: &crate::Session,
	object: &Object<T>,
	r#type: pkcs11_sys::CK_ATTRIBUTE_TYPE,
	C_GetAttributeValue: pkcs11_sys::CK_C_GetAttributeValue,
) -> Result<Vec<u8>, GetKeyParametersError> {
	// Per the docs of C_GetAttributeValue, it is legal to call it with pValue == NULL and ulValueLen == 0.
	// In this case it will set ulValueLen to the size of buffer it needs and return CKR_OK.

	let mut attribute = pkcs11_sys::CK_ATTRIBUTE {
		r#type,
		pValue: std::ptr::null_mut(),
		ulValueLen: 0,
	};

	let result =
		C_GetAttributeValue(
			session.handle,
			object.handle,
			&mut attribute,
			1,
		);
	if result != pkcs11_sys::CKR_OK {
		return Err(GetKeyParametersError::GetAttributeValueFailed(result));
	}

	let mut buf = vec![0_u8; std::convert::TryInto::try_into(attribute.ulValueLen).expect("CK_ULONG -> usize")];
	attribute.pValue = buf.as_mut_ptr() as _;

	let result =
		C_GetAttributeValue(
			session.handle,
			object.handle,
			&mut attribute,
			1,
		);
	if result != pkcs11_sys::CKR_OK {
		return Err(GetKeyParametersError::GetAttributeValueFailed(result));
	}

	Ok(buf)
}