// SPDX-License-Identifier: MIT OR Apache-2.0
//! A small `.sbs` package for the tests, written by hand for them from the published structure of the format; it is
//! not taken from any other program.
#![allow(dead_code)]

pub fn tiny_png() -> String {
    let px: Vec<[f32; 4]> = (0..16).map(|i| [i as f32 / 15.0, 0.5, 0.25, 1.0]).collect();
    materialcraft::graph::b64_encode(&materialcraft::png::encode(4, 4, &px, 8))
}

pub fn p_float(name: &str, v: &str) -> String {
    format!("<parameter><name v=\"{name}\"/><relativeTo v=\"0\"/><paramValue><constantValueFloat1 v=\"{v}\"/></paramValue></parameter>")
}
pub fn p_int(name: &str, v: i32) -> String {
    format!("<parameter><name v=\"{name}\"/><paramValue><constantValueInt32 v=\"{v}\"/></paramValue></parameter>")
}
pub fn node(uid: u32, x: f32, body: &str, conns: &str) -> String {
    format!("<compNode><uid v=\"{uid}\"/><GUILayout><gpos v=\"{x} 0 0\"/></GUILayout><compOutputs><compOutput><uid v=\"{}\"/><comptype v=\"1\"/></compOutput></compOutputs><connections>{conns}</connections><compImplementation>{body}</compImplementation></compNode>", uid * 10)
}
pub fn conn(slot: &str, from: u32) -> String {
    format!("<connection><identifier v=\"{slot}\"/><connRef v=\"{from}\"/><connRefOutput v=\"{}\"/></connection>", from * 10)
}
pub fn filter(name: &str, params: &str) -> String {
    format!("<compFilter><filter v=\"{name}\"/><parameters>{params}</parameters></compFilter>")
}

/// opacity = roughness_amount * 2, written as a function graph
pub fn dyn_opacity() -> String {
    "<parameter><name v=\"opacitymult\"/><paramValue><dynamicValue><rootnode v=\"3\"/><paramNodes>\
     <paramNode><uid v=\"1\"/><function v=\"get_float1\"/><funcDatas><funcData><name v=\"get_float1\"/><constantValue><constantValueString v=\"roughness_amount\"/></constantValue></funcData></funcDatas></paramNode>\
     <paramNode><uid v=\"2\"/><function v=\"const_float1\"/><funcDatas><funcData><name v=\"const_float1\"/><constantValue><constantValueFloat1 v=\"2\"/></constantValue></funcData></funcDatas></paramNode>\
     <paramNode><uid v=\"3\"/><function v=\"mul\"/><connections><connection><identifier v=\"a\"/><connRef v=\"1\"/></connection><connection><identifier v=\"b\"/><connRef v=\"2\"/></connection></connections></paramNode>\
     </paramNodes></dynamicValue></paramValue></parameter>"
        .into()
}

/// per pixel: colour of input 0 at the current position, mixed with a float4 built from parts
pub fn dyn_perpixel() -> String {
    "<parameter><name v=\"perpixel\"/><paramValue><dynamicValue><rootnode v=\"9\"/><paramNodes>\
     <paramNode><uid v=\"1\"/><function v=\"get_float2\"/><funcDatas><funcData><name v=\"get_float2\"/><constantValue><constantValueString v=\"$pos\"/></constantValue></funcData></funcDatas></paramNode>\
     <paramNode><uid v=\"2\"/><function v=\"samplecol\"/><funcDatas><funcData><name v=\"samplecol\"/><constantValue><constantValueInt2 v=\"0 1\"/></constantValue></funcData></funcDatas><connections><connection><identifier v=\"pos\"/><connRef v=\"1\"/></connection></connections></paramNode>\
     <paramNode><uid v=\"3\"/><function v=\"const_float1\"/><funcDatas><funcData><name v=\"const_float1\"/><constantValue><constantValueFloat1 v=\"0.5\"/></constantValue></funcData></funcDatas></paramNode>\
     <paramNode><uid v=\"4\"/><function v=\"vector2\"/><connections><connection><identifier v=\"componentsin\"/><connRef v=\"3\"/></connection><connection><identifier v=\"componentslast\"/><connRef v=\"3\"/></connection></connections></paramNode>\
     <paramNode><uid v=\"5\"/><function v=\"vector4\"/><connections><connection><identifier v=\"componentsin\"/><connRef v=\"4\"/></connection><connection><identifier v=\"componentslast\"/><connRef v=\"4\"/></connection></connections></paramNode>\
     <paramNode><uid v=\"6\"/><function v=\"get_float1\"/><funcDatas><funcData><name v=\"get_float1\"/><constantValue><constantValueString v=\"roughness_amount\"/></constantValue></funcData></funcDatas></paramNode>\
     <paramNode><uid v=\"9\"/><function v=\"lerp\"/><connections><connection><identifier v=\"a\"/><connRef v=\"2\"/></connection><connection><identifier v=\"b\"/><connRef v=\"5\"/></connection><connection><identifier v=\"x\"/><connRef v=\"6\"/></connection></connections></paramNode>\
     </paramNodes></dynamicValue></paramValue></parameter>"
        .into()
}

pub fn fixture() -> String {
    let main_nodes = [
        node(1, 0.0, &filter("uniform", "<parameter><name v=\"outputcolor\"/><paramValue><constantValueFloat4 v=\"0.8 0.2 0.1 1\"/></paramValue></parameter>"), ""),
        node(2, 0.0, &format!("<compInstance><path v=\"pkg:///perlin_noise?dependency=2\"/><parameters>{}{}</parameters></compInstance>", p_float("scale", "8"), p_float("not_a_real_param", "3")), ""),
        node(3, 150.0, &filter("blend", &(p_int("blendingmode", 3) + &dyn_opacity())), &(conn("source", 1) + &conn("destination", 2))),
        node(4, 150.0, &filter("levels", "<parameter><name v=\"levelinmid\"/><paramValue><constantValueFloat4 v=\"0.25 0.25 0.25 0.25\"/></paramValue></parameter>"), &conn("input1", 2)),
        node(5, 300.0, &filter("normal", &p_float("intensity", "1.5")), &conn("input1", 4)),
        node(6, -150.0, "<compInputBridge><entry v=\"100\"/></compInputBridge>", ""),
        node(7, 450.0, "<compOutputBridge><output v=\"201\"/></compOutputBridge>", &conn("inputNodeOutput", 3)),
        node(8, 450.0, "<compOutputBridge><output v=\"202\"/></compOutputBridge>", "<connection><identifier v=\"inputNodeOutput\"/><connRef v=\"11\"/><connRefOutput v=\"1101\"/></connection>"),
        node(9, 450.0, "<compOutputBridge><output v=\"203\"/></compOutputBridge>", &conn("inputNodeOutput", 5)),
        node(10, 300.0, &format!("<compFilter><filter v=\"pixelprocessor\"/><parameters>{}</parameters></compFilter>", dyn_perpixel()), &conn("input", 2)),
        // an instance of the local graph "Sub", with a parameter and a named output
        format!(
            "<compNode><uid v=\"11\"/><GUILayout><gpos v=\"300 120 0\"/></GUILayout><compOutputs><compOutput><uid v=\"1101\"/></compOutput></compOutputs><connections>{}</connections><compImplementation><compInstance><path v=\"pkg:///Folder/Sub?dependency=1\"/><parameters>{}</parameters><outputBridgings><outputBridging><uid v=\"1101\"/><identifier v=\"result\"/></outputBridging></outputBridgings></compInstance></compImplementation></compNode>",
            conn("img", 4),
            p_float("amount", "0.7")
        ),
        node(12, 300.0, "<compFilter><filter v=\"gradient\"/><parameters/><paramsArrays><paramsArray><name v=\"gradientrgba\"/><uid v=\"1\"/><paramsArrayCells>\
            <paramsArrayCell><uid v=\"1\"/><parameters><parameter><name v=\"position\"/><paramValue><constantValueFloat1 v=\"0\"/></paramValue></parameter><parameter><name v=\"value\"/><paramValue><constantValueFloat4 v=\"0 0 0.5 1\"/></paramValue></parameter></parameters></paramsArrayCell>\
            <paramsArrayCell><uid v=\"2\"/><parameters><parameter><name v=\"position\"/><paramValue><constantValueFloat1 v=\"1\"/></paramValue></parameter><parameter><name v=\"value\"/><paramValue><constantValueFloat4 v=\"1 0.9 0.2 1\"/></paramValue></parameter></parameters></paramsArrayCell>\
            </paramsArrayCells></paramsArray></paramsArrays></compFilter>", &conn("input1", 2)),
        node(13, 450.0, "<compFilter><filter v=\"fxmaps\"/><parameters/><paramsGraphs><paramsGraph><name v=\"fxmap\"/><uid v=\"1\"/></paramsGraph></paramsGraphs></compFilter>", ""),
        node(14, 450.0, "<compInstance><path v=\"pkg:///some_unknown_thing?dependency=2\"/></compInstance>", &conn("input", 2)),
        node(15, 0.0, &filter("bitmap", "<parameter><name v=\"bitmapresourcepath\"/><paramValue><constantValueString v=\"pkg:///Resources/tiny?dependency=1\"/></paramValue></parameter>"), ""),
        node(16, 150.0, &filter("transformation", "<parameter><name v=\"matrix22\"/><paramValue><constantValueFloat4 v=\"0.5 0 0 0.5\"/></paramValue></parameter><parameter><name v=\"offset\"/><paramValue><constantValueFloat2 v=\"0.25 0\"/></paramValue></parameter>"), &conn("input1", 15)),
        node(17, 150.0, &filter("shuffle", &(p_int("channelred", 2) + &p_int("channelblue", 0))), &(conn("input1", 1) + &conn("input2", 2))),
    ]
    .join("");
    let sub_nodes = [
        node(1, 0.0, "<compInputBridge><entry v=\"50\"/></compInputBridge>", ""),
        node(
            2,
            150.0,
            &filter(
                "levels",
                "<parameter><name v=\"levelouthigh\"/><paramValue><dynamicValue><rootnode v=\"1\"/><paramNodes><paramNode><uid v=\"1\"/><function v=\"get_float1\"/><funcDatas><funcData><name v=\"get_float1\"/><constantValue><constantValueString v=\"amount\"/></constantValue></funcData></funcDatas></paramNode></paramNodes></dynamicValue></paramValue></parameter>",
            ),
            &conn("input1", 1),
        ),
        node(3, 300.0, "<compOutputBridge><output v=\"60\"/></compOutputBridge>", &conn("inputNodeOutput", 2)),
    ]
    .join("");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<package><identifier v=\"Test\"/><formatVersion v=\"1.1.0.0\"/>\
         <dependencies><dependency><filename v=\"?himself\"/><uid v=\"1\"/><type v=\"package\"/></dependency>\
         <dependency><filename v=\"sbs://library_stuff.sbs\"/><uid v=\"2\"/><type v=\"package\"/></dependency></dependencies>\
         <content>\
         <graph><identifier v=\"Main\"/><uid v=\"1000\"/><attributes><label v=\"Main material\"/></attributes>\
           <paramInputs>\
             <paramInput><identifier v=\"mask_in\"/><uid v=\"100\"/><type v=\"2\"/></paramInput>\
             <paramInput><identifier v=\"roughness_amount\"/><uid v=\"101\"/><type v=\"256\"/><defaultValue><constantValueFloat1 v=\"0.4\"/></defaultValue><defaultWidget><name v=\"slider\"/><options><option><name v=\"min\"/><value v=\"0\"/></option><option><name v=\"max\"/><value v=\"1\"/></option></options></defaultWidget></paramInput>\
           </paramInputs>\
           <graphOutputs>\
             <graphoutput><identifier v=\"basecolor\"/><uid v=\"201\"/><usages><usage><components v=\"RGBA\"/><name v=\"baseColor\"/></usage></usages></graphoutput>\
             <graphoutput><identifier v=\"rough\"/><uid v=\"202\"/><usages><usage><components v=\"R\"/><name v=\"roughness\"/></usage></usages></graphoutput>\
             <graphoutput><identifier v=\"normal\"/><uid v=\"203\"/><usages><usage><components v=\"RGBA\"/><name v=\"normal\"/></usage></usages></graphoutput>\
           </graphOutputs>\
           <compNodes>{main_nodes}</compNodes>\
           <baseParameters><parameter><name v=\"outputsize\"/><relativeTo v=\"0\"/><paramValue><constantValueInt2 v=\"7 7\"/></paramValue></parameter></baseParameters>\
           <GUIObjects><GUIObject><type v=\"COMMENT\"/><GUIName v=\"Hand written test &amp; notes\"/><uid v=\"5\"/><GUILayout><gpos v=\"10 -80 0\"/><size v=\"200 60\"/></GUILayout></GUIObject></GUIObjects>\
         </graph>\
         <group><identifier v=\"Folder\"/><uid v=\"7\"/><content>\
           <graph><identifier v=\"Sub\"/><uid v=\"2000\"/>\
             <paramInputs>\
               <paramInput><identifier v=\"img\"/><uid v=\"50\"/><type v=\"2\"/></paramInput>\
               <paramInput><identifier v=\"amount\"/><uid v=\"51\"/><type v=\"256\"/><defaultValue><constantValueFloat1 v=\"0.5\"/></defaultValue></paramInput>\
             </paramInputs>\
             <graphOutputs><graphoutput><identifier v=\"result\"/><uid v=\"60\"/></graphoutput></graphOutputs>\
             <compNodes>{sub_nodes}</compNodes>\
           </graph>\
         </content></group>\
         <function><identifier v=\"double_it\"/><uid v=\"3000\"/><paramInputs><paramInput><identifier v=\"x\"/><uid v=\"1\"/><type v=\"256\"/><defaultValue><constantValueFloat1 v=\"1\"/></defaultValue></paramInput></paramInputs>\
           <paramValue><dynamicValue><rootnode v=\"3\"/><paramNodes>\
             <paramNode><uid v=\"1\"/><function v=\"get_float1\"/><funcDatas><funcData><name v=\"get_float1\"/><constantValue><constantValueString v=\"x\"/></constantValue></funcData></funcDatas></paramNode>\
             <paramNode><uid v=\"2\"/><function v=\"const_float1\"/><funcDatas><funcData><name v=\"const_float1\"/><constantValue><constantValueFloat1 v=\"2\"/></constantValue></funcData></funcDatas></paramNode>\
             <paramNode><uid v=\"3\"/><function v=\"mul\"/><connections><connection><identifier v=\"a\"/><connRef v=\"1\"/></connection><connection><identifier v=\"b\"/><connRef v=\"2\"/></connection></connections></paramNode>\
           </paramNodes></dynamicValue></paramValue></function>\
         <group><identifier v=\"Resources\"/><uid v=\"8\"/><content>\
           <resource><identifier v=\"tiny\"/><uid v=\"9\"/><type v=\"bitmap\"/><format v=\"png\"/><source><embedded><format v=\"rawbase64\"/><strdata v=\"{png}\"/></embedded></source></resource>\
         </content></group>\
         </content></package>",
        png = tiny_png()
    )
}

