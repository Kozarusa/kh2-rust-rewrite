//! The particle program ("ppp") table. Program kinds in the data index this table; the names are
//! the game's own program names as listed by OpenKH `OpenKh.Kh2/DpdPData.cs` (`PppProgramEnum`),
//! and the order matches the USA executable's program table.

pub const NAMES: &[&str] = &[
    "", "pppDummyFunc", "pppKeThRes", "pppKeThRes8", "pppKeThRes8x4", "pppKeThRes8x128", "pppKeThRes16",
    "pppKeThRes16x4", "pppKeThRes16x64", "pppKeThRes24", "pppKeThRes24x4", "pppKeThRes32", "pppKeThRes32x4",
    "pppKeThRes32x32", "pppKeThRes40", "pppKeThRes40x4", "pppKeThRes48", "pppKeThRes48x4", "pppKeThRes64",
    "pppKeThRes64x4", "pppKeThRes64x16", "pppKeThRes128", "pppKeThRes128x4", "pppKeThRes128x8",
    "pppKeThRes255", "pppKeThRes255x4", "pppKeHitBall", "pppKeGrvTgt", "pppAccele", "pppAngAccele",
    "pppSclAccele", "pppColAccele", "pppEiWfacc", "pppKeHmgEff", "pppKeGrvEff", "pppKeHitChkPxB", "pppMove",
    "pppAngMove", "pppSclMove", "pppColMove", "pppPoint", "pppAngle", "pppScale", "pppColor", "pppPObjPoint",
    "pppVertexAttend", "pppEiWindFun", "pppKeMvYpEff", "pppKeDrct", "pppVtMime", "pppKeTkFade",
    "pppRandFloat", "pppRandUpFloat", "pppRandDownFloat", "pppRandChar", "pppRandUpChar", "pppRandDownChar",
    "pppRandShort", "pppRandUpShort", "pppRandDownShort", "pppRandInt", "pppRandUpInt", "pppRandDownInt",
    "pppRandFV", "pppRandUpFV", "pppRandDownFV", "pppRandIV", "pppRandUpIV", "pppRandDownIV", "pppRandCV",
    "pppRandUpCV", "pppRandDownCV", "pppRandHCV", "pppRandUpHCV", "pppRandDownHCV", "pppSRandFV",
    "pppSRandUpFV", "pppSRandDownFV", "pppSRandCV", "pppSRandUpCV", "pppSRandDownCV", "pppSRandHCV",
    "pppSRandUpHCV", "pppSRandDownHCV", "pppSMatrix", "pppKeMatSN", "pppMatrix", "pppMatrixXYZ",
    "pppMatrixXZY", "pppMatrixYZX", "pppMatrixYXZ", "pppMatrixZXY", "pppMatrixZYX", "pppMatrixLoc",
    "pppMatrixScl", "pppMatrixFront", "pppRyjMatrixNoRot", "pppKeMatPht", "pppRyjMatrixWorld",
    "pppRyjMatrixWorldXYZ", "pppRyjMatrixWorldXZY", "pppRyjMatrixWorldYXZ", "pppRyjMatrixWorldYZX",
    "pppRyjMatrixWorldZXY", "pppRyjMatrixWorldZYX", "pppParMatrix", "pppKeParMatR", "pppChrSclXYZMatrix",
    "pppChrSclXZMatrix", "pppChrSclYMatrix", "pppChrYSclXYZMatrix", "pppChrXSclXYZMatrix", "pppDrawMatrix",
    "pppDrawMatrixFront", "pppDrawMatrixNoRot", "pppDrawMatrixWood", "pppKeDMat", "pppKeDMatFr",
    "pppKeDMatPht", "pppKeDMatPhtFr", "pppRyjDrawMatrixWorld", "pppRyjDrawMatrixWorldFront",
    "pppRyjDrawMatrixWorldNoRot", "pppRyjDrawMatrixWorldWood", "pppKeZCrct", "pppKeZCrctShp", "pppKeThTp",
    "pppKeThTp2", "pppKeThSft", "pppKeTh", "pppDrawMdlBS", "pppDrawMdl", "pppDrawMdlSemi", "pppDrawMdlTs",
    "pppDrawMdl2", "pppDrawMdlSemi2", "pppDrawMdlTs2", "pppDrawMdl3", "pppDrawMdlSemi3", "pppDrawMdlTs3",
    "pppDrawMdlSea", "pppDrawShape", "pppDrawShapeX", "pppDrawShapeField", "pppKeMdlBmp", "pppKeMdlDtt",
    "pppKeMdlTfd", "pppKeMdlTfdUv", "pppKeMdlTfd2", "pppKeMdlTfdUv2", "pppKeMdlTfd3", "pppKeMdlTfdUv3",
    "pppKeShpTail", "pppKeShpTailX", "pppKeShpTail2", "pppKeShpTail2X", "pppKeShpTail3", "pppKeShpTail3X",
    "pppKeShpTailPht", "pppKeShpTailLc", "pppKeShpDtt", "pppRyjDrawShipoly", "pppDrawHook", "pppSDMatrix",
    "pppSCMatrix", "pppWMatrix", "pppWMatrixXYZ", "pppWMatrixXZY", "pppWMatrixYZX", "pppWMatrixYXZ",
    "pppWMatrixZXY", "pppWMatrixZYX", "pppPointAp", "pppPointRAp", "pppVertexAp", "pppSegmentAp",
    "pppFaceAp", "pppVertexApLc", "pppVertexApAt", "pppKeBornRnd", "pppKeBornRnd2", "pppKeBornRnd3",
    "pppKeBornRnd4", "pppKeBornRnd5", "pppKeBornRnd6", "pppKeBornPtCmpl", "pppKeHitBorn", "pppKeThHitBorn",
    "pppKeLnsLpSft", "pppKeLnsLp", "pppKeLnsArnd", "pppKeLnsClm", "pppKeLnsCrn", "pppKeLnsFls",
    "pppKeAcmSolid", "pppKeHitChk", "pppKeThLz", "pppKeThCp", "pppKeThCpSft", "pppKeAccSpdSv",
    "pppRyjDrawShipolyBone", "pppRyjMegaBirth", "pppRyjMngFlag", "pppRyjMegaBirthPrize", "pppRyjMegaPlace",
    "pppRyjMegaPlaceShape", "pppRyjMegaPlaceModel", "pppRyjMegaPlaceLamp", "pppRyjMegaBirthModel",
    "pppRyjDrawKekoto", "pppMoveLoop", "pppAngMoveLoop", "pppSclMoveLoop", "pppPointLoop", "pppAngleLoop",
    "pppScaleLoop", "pppMatrixLoop", "pppDrawMatrixLoop", "pppDrawMdlLoop", "pppDrawMdlLoopZ", "pppRyjLight",
    "pppKeLnsLpT", "pppKeLnsArndT", "pppKeLnsClmT", "pppKeLnsCrnT", "pppKeLnsFlsT",
    "pppRyjDrawMatrixWorldDtt", "pppRyjDrawMatrixWorldDttFr", "pppRyjMegaBirthFilter",
    "pppRyjMegaBirthModelFilter", "pppMatrixLoopXYZ", "pppMatrixLoopXZY", "pppMatrixLoopYXZ",
    "pppMatrixLoopYZX", "pppMatrixLoopZXY", "pppMatrixLoopZYX", "pppVertexApLcLoop", "pppSnoScaleAll",
    "pppSnoMdlDttNorm", "pppRyjMegaBirthUserCtrl", "pppRyjMegaBirthUserCtrl2", "pppEiDrawShipoly",
    "pppRyjMegaBirthUserCtrl3",
];

pub fn name(kind: u32) -> &'static str {
    NAMES.get(kind as usize).copied().filter(|s| !s.is_empty()).unwrap_or("?")
}

/// Program kinds used by name in the runtime.
pub mod kind {
    pub const ACCELE: u32 = 28;
    pub const ANG_ACCELE: u32 = 29;
    pub const SCL_ACCELE: u32 = 30;
    pub const COL_ACCELE: u32 = 31;
    pub const MOVE: u32 = 36;
    pub const ANG_MOVE: u32 = 37;
    pub const SCL_MOVE: u32 = 38;
    pub const COL_MOVE: u32 = 39;
    pub const POINT: u32 = 40;
    pub const ANGLE: u32 = 41;
    pub const SCALE: u32 = 42;
    pub const COLOR: u32 = 43;
    pub const MATRIX: u32 = 86;
    pub const MATRIX_XYZ: u32 = 87;
    pub const MATRIX_SCL: u32 = 94;
    pub const DRAW_MATRIX: u32 = 112;
    pub const DRAW_MATRIX_FRONT: u32 = 113;
    pub const RYJ_DRAW_SHIPOLY_BONE: u32 = 200;
    pub const RYJ_MEGA_BIRTH: u32 = 201;
    pub const RYJ_MEGA_BIRTH_MODEL: u32 = 208;
    pub const EI_DRAW_SHIPOLY: u32 = 241;
}
