use serde::Serialize;

#[derive(Serialize)]
pub struct SoapResponseBody<T: Serialize> {
    pub action: T,
}

#[derive(Serialize)]
pub struct SoapResponse<T: Serialize> {
    #[serde(rename = "s:Envelope")]
    pub envelope: SoapResponseEnvelope<T>,
}

#[derive(Serialize)]
pub struct SoapResponseEnvelope<T: Serialize> {
    #[serde(rename = "s:Body")]
    pub body: SoapResponseBody<T>,
}