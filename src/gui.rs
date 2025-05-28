#![allow(clippy::use_self)]

use dashmap::DashMap;
use eframe::{App, CreationContext};
use egui::Color32;
use futures_util::SinkExt;
use futures_util::StreamExt;
use once_cell::sync::OnceCell;
use salvo::prelude::*;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::watch;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use which::which;

const STRING_COLOR: Color32 = Color32::from_rgb(0x00, 0xb0, 0x00);
const NUMBER_COLOR: Color32 = Color32::from_rgb(0xb0, 0x00, 0x00);
const IMAGE_COLOR: Color32 = Color32::from_rgb(0xb0, 0x00, 0xb0);
const UNTYPED_COLOR: Color32 = Color32::from_rgb(0xb0, 0xb0, 0xb0);

#[cfg(feature = "uses_funny")]
use crate::jokes;

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub enum DemoNode {
    /// Node with single input.
    /// Displays the value of the input.
    Sink,

    /// Value node with a single output.
    /// The value is editable in UI.
    Number(f64),

    /// Value node with a single output.
    String(String),

    /// Converts URI to Image
    ShowImage(String),

    /// Expression node with a single output.
    /// It has number of inputs equal to number of variables in the expression.
    ExprNode(ExprNode),
}

impl DemoNode {
    const fn name(&self) -> &str {
        match self {
            DemoNode::Sink => "Sink",
            DemoNode::Number(_) => "Number",
            DemoNode::String(_) => "String",
            DemoNode::ShowImage(_) => "ShowImage",
            DemoNode::ExprNode(_) => "ExprNode",
        }
    }

    fn number_out(&self) -> f64 {
        match self {
            DemoNode::Number(value) => *value,
            DemoNode::ExprNode(expr_node) => expr_node.eval(),
            _ => unreachable!(),
        }
    }

    fn number_in(&mut self, idx: usize) -> &mut f64 {
        match self {
            DemoNode::ExprNode(expr_node) => &mut expr_node.values[idx - 1],
            _ => unreachable!(),
        }
    }

    fn label_in(&mut self, idx: usize) -> &str {
        match self {
            DemoNode::ShowImage(_) if idx == 0 => "URL",
            DemoNode::ExprNode(expr_node) => &expr_node.bindings[idx - 1],
            _ => unreachable!(),
        }
    }

    fn string_out(&self) -> &str {
        match self {
            DemoNode::String(value) => value,
            _ => unreachable!(),
        }
    }

    fn string_in(&mut self) -> &mut String {
        match self {
            DemoNode::ShowImage(uri) => uri,
            DemoNode::ExprNode(expr_node) => &mut expr_node.text,
            _ => unreachable!(),
        }
    }

    fn expr_node(&mut self) -> &mut ExprNode {
        match self {
            DemoNode::ExprNode(expr_node) => expr_node,
            _ => unreachable!(),
        }
    }
}

struct DemoViewer;

// impl SnarlViewer<DemoNode> for DemoViewer {
//     #[inline]
//     fn connect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<DemoNode>) {
//         // Validate connection
//         #[allow(clippy::match_same_arms)] // For match clarity
//         match (&snarl[from.id.node], &snarl[to.id.node]) {
//             (DemoNode::Sink, _) => {
//                 unreachable!("Sink node has no outputs")
//             }
//             (_, DemoNode::Sink) => {}
//             (_, DemoNode::Number(_)) => {
//                 unreachable!("Number node has no inputs")
//             }
//             (_, DemoNode::String(_)) => {
//                 unreachable!("String node has no inputs")
//             }
//             (DemoNode::Number(_), DemoNode::ShowImage(_)) => {
//                 return;
//             }
//             (DemoNode::ShowImage(_), DemoNode::ShowImage(_)) => {
//                 return;
//             }
//             (DemoNode::String(_), DemoNode::ShowImage(_)) => {}
//             (DemoNode::ExprNode(_), DemoNode::ExprNode(_)) if to.id.input == 0 => {
//                 return;
//             }
//             (DemoNode::ExprNode(_), DemoNode::ExprNode(_)) => {}
//             (DemoNode::Number(_), DemoNode::ExprNode(_)) if to.id.input == 0 => {
//                 return;
//             }
//             (DemoNode::Number(_), DemoNode::ExprNode(_)) => {}
//             (DemoNode::String(_), DemoNode::ExprNode(_)) if to.id.input == 0 => {}
//             (DemoNode::String(_), DemoNode::ExprNode(_)) => {
//                 return;
//             }
//             (DemoNode::ShowImage(_), DemoNode::ExprNode(_)) => {
//                 return;
//             }
//             (DemoNode::ExprNode(_), DemoNode::ShowImage(_)) => {
//                 return;
//             }
//         }

//         for &remote in &to.remotes {
//             snarl.disconnect(remote, to.id);
//         }

//         snarl.connect(from.id, to.id);
//     }

//     fn title(&mut self, node: &DemoNode) -> String {
//         match node {
//             DemoNode::Sink => "Sink".to_owned(),
//             DemoNode::Number(_) => "Number".to_owned(),
//             DemoNode::String(_) => "String".to_owned(),
//             DemoNode::ShowImage(_) => "Show image".to_owned(),
//             DemoNode::ExprNode(_) => "Expr".to_owned(),
//         }
//     }

//     fn inputs(&mut self, node: &DemoNode) -> usize {
//         match node {
//             DemoNode::Sink | DemoNode::ShowImage(_) => 1,
//             DemoNode::Number(_) | DemoNode::String(_) => 0,
//             DemoNode::ExprNode(expr_node) => 1 + expr_node.bindings.len(),
//         }
//     }

//     fn outputs(&mut self, node: &DemoNode) -> usize {
//         match node {
//             DemoNode::Sink => 0,
//             DemoNode::Number(_)
//             | DemoNode::String(_)
//             | DemoNode::ShowImage(_)
//             | DemoNode::ExprNode(_) => 1,
//         }
//     }

//     #[allow(clippy::too_many_lines)]
//     #[allow(refining_impl_trait)]
//     fn show_input(&mut self, pin: &InPin, ui: &mut Ui, snarl: &mut Snarl<DemoNode>) -> PinInfo {
//         match snarl[pin.id.node] {
//             DemoNode::Sink => {
//                 assert_eq!(pin.id.input, 0, "Sink node has only one input");

//                 match &*pin.remotes {
//                     [] => {
//                         ui.label("None");
//                         PinInfo::circle().with_fill(UNTYPED_COLOR)
//                     }
//                     [remote] => match snarl[remote.node] {
//                         DemoNode::Sink => unreachable!("Sink node has no outputs"),
//                         DemoNode::Number(value) => {
//                             assert_eq!(remote.output, 0, "Number node has only one output");
//                             ui.label(format_float(value));
//                             PinInfo::circle().with_fill(NUMBER_COLOR)
//                         }
//                         DemoNode::String(ref value) => {
//                             assert_eq!(remote.output, 0, "String node has only one output");
//                             ui.label(format!("{value:?}"));

//                             PinInfo::circle().with_fill(STRING_COLOR).with_wire_style(
//                                 WireStyle::AxisAligned {
//                                     corner_radius: 10.0,
//                                 },
//                             )
//                         }
//                         DemoNode::ExprNode(ref expr) => {
//                             assert_eq!(remote.output, 0, "Expr node has only one output");
//                             ui.label(format_float(expr.eval()));
//                             PinInfo::circle().with_fill(NUMBER_COLOR)
//                         }
//                         DemoNode::ShowImage(ref uri) => {
//                             assert_eq!(remote.output, 0, "ShowImage node has only one output");

//                             let image = egui::Image::new(uri).show_loading_spinner(true);
//                             ui.add(image);

//                             PinInfo::circle().with_fill(IMAGE_COLOR)
//                         }
//                     },
//                     _ => unreachable!("Sink input has only one wire"),
//                 }
//             }
//             DemoNode::Number(_) => {
//                 unreachable!("Number node has no inputs")
//             }
//             DemoNode::String(_) => {
//                 unreachable!("String node has no inputs")
//             }
//             DemoNode::ShowImage(_) => match &*pin.remotes {
//                 [] => {
//                     let input = snarl[pin.id.node].string_in();
//                     egui::TextEdit::singleline(input)
//                         .clip_text(false)
//                         .desired_width(0.0)
//                         .margin(ui.spacing().item_spacing)
//                         .show(ui);
//                     PinInfo::circle().with_fill(STRING_COLOR).with_wire_style(
//                         WireStyle::AxisAligned {
//                             corner_radius: 10.0,
//                         },
//                     )
//                 }
//                 [remote] => {
//                     let new_value = snarl[remote.node].string_out().to_owned();

//                     egui::TextEdit::singleline(&mut &*new_value)
//                         .clip_text(false)
//                         .desired_width(0.0)
//                         .margin(ui.spacing().item_spacing)
//                         .show(ui);

//                     let input = snarl[pin.id.node].string_in();
//                     *input = new_value;

//                     PinInfo::circle().with_fill(STRING_COLOR).with_wire_style(
//                         WireStyle::AxisAligned {
//                             corner_radius: 10.0,
//                         },
//                     )
//                 }
//                 _ => unreachable!("Sink input has only one wire"),
//             },
//             DemoNode::ExprNode(_) if pin.id.input == 0 => {
//                 let changed = match &*pin.remotes {
//                     [] => {
//                         let input = snarl[pin.id.node].string_in();
//                         let r = egui::TextEdit::singleline(input)
//                             .clip_text(false)
//                             .desired_width(0.0)
//                             .margin(ui.spacing().item_spacing)
//                             .show(ui)
//                             .response;

//                         r.changed()
//                     }
//                     [remote] => {
//                         let new_string = snarl[remote.node].string_out().to_owned();

//                         egui::TextEdit::singleline(&mut &*new_string)
//                             .clip_text(false)
//                             .desired_width(0.0)
//                             .margin(ui.spacing().item_spacing)
//                             .show(ui);

//                         let input = snarl[pin.id.node].string_in();
//                         if new_string == *input {
//                             false
//                         } else {
//                             *input = new_string;
//                             true
//                         }
//                     }
//                     _ => unreachable!("Expr pins has only one wire"),
//                 };

//                 if changed {
//                     let expr_node = snarl[pin.id.node].expr_node();

//                     if let Ok(expr) = syn::parse_str(&expr_node.text) {
//                         expr_node.expr = expr;

//                         let values = Iterator::zip(
//                             expr_node.bindings.iter().map(String::clone),
//                             expr_node.values.iter().copied(),
//                         )
//                         .collect::<HashMap<String, f64>>();

//                         let mut new_bindings = Vec::new();
//                         expr_node.expr.extend_bindings(&mut new_bindings);

//                         let old_bindings =
//                             std::mem::replace(&mut expr_node.bindings, new_bindings.clone());

//                         let new_values = new_bindings
//                             .iter()
//                             .map(|name| values.get(&**name).copied().unwrap_or(0.0))
//                             .collect::<Vec<_>>();

//                         expr_node.values = new_values;

//                         let old_inputs = (0..old_bindings.len())
//                             .map(|idx| {
//                                 snarl.in_pin(InPinId {
//                                     node: pin.id.node,
//                                     input: idx + 1,
//                                 })
//                             })
//                             .collect::<Vec<_>>();

//                         for (idx, name) in old_bindings.iter().enumerate() {
//                             let new_idx =
//                                 new_bindings.iter().position(|new_name| *new_name == *name);

//                             match new_idx {
//                                 None => {
//                                     snarl.drop_inputs(old_inputs[idx].id);
//                                 }
//                                 Some(new_idx) if new_idx != idx => {
//                                     let new_in_pin = InPinId {
//                                         node: pin.id.node,
//                                         input: new_idx,
//                                     };
//                                     for &remote in &old_inputs[idx].remotes {
//                                         snarl.disconnect(remote, old_inputs[idx].id);
//                                         snarl.connect(remote, new_in_pin);
//                                     }
//                                 }
//                                 _ => {}
//                             }
//                         }
//                     }
//                 }
//                 PinInfo::circle()
//                     .with_fill(STRING_COLOR)
//                     .with_wire_style(WireStyle::AxisAligned {
//                         corner_radius: 10.0,
//                     })
//             }
//             DemoNode::ExprNode(ref expr_node) => {
//                 if pin.id.input <= expr_node.bindings.len() {
//                     match &*pin.remotes {
//                         [] => {
//                             let node = &mut snarl[pin.id.node];
//                             ui.label(node.label_in(pin.id.input));
//                             ui.add(egui::DragValue::new(node.number_in(pin.id.input)));
//                             PinInfo::circle().with_fill(NUMBER_COLOR)
//                         }
//                         [remote] => {
//                             let new_value = snarl[remote.node].number_out();
//                             let node = &mut snarl[pin.id.node];
//                             ui.label(node.label_in(pin.id.input));
//                             ui.label(format_float(new_value));
//                             *node.number_in(pin.id.input) = new_value;
//                             PinInfo::circle().with_fill(NUMBER_COLOR)
//                         }
//                         _ => unreachable!("Expr pins has only one wire"),
//                     }
//                 } else {
//                     ui.label("Removed");
//                     PinInfo::circle().with_fill(Color32::BLACK)
//                 }
//             }
//         }
//     }

//     #[allow(refining_impl_trait)]
//     fn show_output(&mut self, pin: &OutPin, ui: &mut Ui, snarl: &mut Snarl<DemoNode>) -> PinInfo {
//         match snarl[pin.id.node] {
//             DemoNode::Sink => {
//                 unreachable!("Sink node has no outputs")
//             }
//             DemoNode::Number(ref mut value) => {
//                 assert_eq!(pin.id.output, 0, "Number node has only one output");
//                 ui.add(egui::DragValue::new(value));
//                 PinInfo::circle().with_fill(NUMBER_COLOR)
//             }
//             DemoNode::String(ref mut value) => {
//                 assert_eq!(pin.id.output, 0, "String node has only one output");
//                 let edit = egui::TextEdit::singleline(value)
//                     .clip_text(false)
//                     .desired_width(0.0)
//                     .margin(ui.spacing().item_spacing);
//                 ui.add(edit);
//                 PinInfo::circle()
//                     .with_fill(STRING_COLOR)
//                     .with_wire_style(WireStyle::AxisAligned {
//                         corner_radius: 10.0,
//                     })
//             }
//             DemoNode::ExprNode(ref expr_node) => {
//                 let value = expr_node.eval();
//                 assert_eq!(pin.id.output, 0, "Expr node has only one output");
//                 ui.label(format_float(value));
//                 PinInfo::circle().with_fill(NUMBER_COLOR)
//             }
//             DemoNode::ShowImage(_) => {
//                 ui.allocate_at_least(egui::Vec2::ZERO, egui::Sense::hover());
//                 PinInfo::circle().with_fill(IMAGE_COLOR)
//             }
//         }
//     }

//     fn has_graph_menu(&mut self, _pos: egui::Pos2, _snarl: &mut Snarl<DemoNode>) -> bool {
//         true
//     }

//     fn show_graph_menu(&mut self, pos: egui::Pos2, ui: &mut Ui, snarl: &mut Snarl<DemoNode>) {
//         ui.label("Add node");
//         if ui.button("Number").clicked() {
//             snarl.insert_node(pos, DemoNode::Number(0.0));
//             ui.close_menu();
//         }
//         if ui.button("Expr").clicked() {
//             snarl.insert_node(pos, DemoNode::ExprNode(ExprNode::new()));
//             ui.close_menu();
//         }
//         if ui.button("String").clicked() {
//             snarl.insert_node(pos, DemoNode::String(String::new()));
//             ui.close_menu();
//         }
//         if ui.button("Show image").clicked() {
//             snarl.insert_node(pos, DemoNode::ShowImage(String::new()));
//             ui.close_menu();
//         }
//         if ui.button("Sink").clicked() {
//             snarl.insert_node(pos, DemoNode::Sink);
//             ui.close_menu();
//         }
//     }

//     fn has_dropped_wire_menu(&mut self, _src_pins: AnyPins, _snarl: &mut Snarl<DemoNode>) -> bool {
//         true
//     }

//     fn show_dropped_wire_menu(
//         &mut self,
//         pos: egui::Pos2,
//         ui: &mut Ui,
//         src_pins: AnyPins,
//         snarl: &mut Snarl<DemoNode>,
//     ) {
//         // In this demo, we create a context-aware node graph menu, and connect a wire
//         // dropped on the fly based on user input to a new node created.
//         //
//         // In your implementation, you may want to define specifications for each node's
//         // pin inputs and outputs and compatibility to make this easier.

//         type PinCompat = usize;
//         const PIN_NUM: PinCompat = 1;
//         const PIN_STR: PinCompat = 2;
//         const PIN_IMG: PinCompat = 4;
//         const PIN_SINK: PinCompat = PIN_NUM | PIN_STR | PIN_IMG;

//         const fn pin_out_compat(node: &DemoNode) -> PinCompat {
//             match node {
//                 DemoNode::Sink => 0,
//                 DemoNode::String(_) => PIN_STR,
//                 DemoNode::ShowImage(_) => PIN_IMG,
//                 DemoNode::Number(_) | DemoNode::ExprNode(_) => PIN_NUM,
//             }
//         }

//         const fn pin_in_compat(node: &DemoNode, pin: usize) -> PinCompat {
//             match node {
//                 DemoNode::Sink => PIN_SINK,
//                 DemoNode::Number(_) | DemoNode::String(_) => 0,
//                 DemoNode::ShowImage(_) => PIN_STR,
//                 DemoNode::ExprNode(_) => {
//                     if pin == 0 {
//                         PIN_STR
//                     } else {
//                         PIN_NUM
//                     }
//                 }
//             }
//         }

//         ui.label("Add node");

//         match src_pins {
//             AnyPins::Out(src_pins) => {
//                 assert!(
//                     src_pins.len() == 1,
//                     "There's no concept of multi-input nodes in this demo"
//                 );

//                 let src_pin = src_pins[0];
//                 let src_out_ty = pin_out_compat(snarl.get_node(src_pin.node).unwrap());
//                 let dst_in_candidates = [
//                     ("Sink", (|| DemoNode::Sink) as fn() -> DemoNode, PIN_SINK),
//                     ("Show Image", || DemoNode::ShowImage(String::new()), PIN_STR),
//                     ("Expr", || DemoNode::ExprNode(ExprNode::new()), PIN_STR),
//                 ];

//                 for (name, ctor, in_ty) in dst_in_candidates {
//                     if src_out_ty & in_ty != 0 && ui.button(name).clicked() {
//                         // Create new node.
//                         let new_node = snarl.insert_node(pos, ctor());
//                         let dst_pin = InPinId {
//                             node: new_node,
//                             input: 0,
//                         };

//                         // Connect the wire.
//                         snarl.connect(src_pin, dst_pin);
//                         ui.close_menu();
//                     }
//                 }
//             }
//             AnyPins::In(pins) => {
//                 let all_src_types = pins.iter().fold(0, |acc, pin| {
//                     acc | pin_in_compat(snarl.get_node(pin.node).unwrap(), pin.input)
//                 });

//                 let dst_out_candidates = [
//                     (
//                         "Number",
//                         (|| DemoNode::Number(0.)) as fn() -> DemoNode,
//                         PIN_NUM,
//                     ),
//                     ("String", || DemoNode::String(String::new()), PIN_STR),
//                     ("Expr", || DemoNode::ExprNode(ExprNode::new()), PIN_NUM),
//                     ("Show Image", || DemoNode::ShowImage(String::new()), PIN_IMG),
//                 ];

//                 for (name, ctor, out_ty) in dst_out_candidates {
//                     if all_src_types & out_ty != 0 && ui.button(name).clicked() {
//                         // Create new node.
//                         let new_node = ctor();
//                         let dst_ty = pin_out_compat(&new_node);

//                         let new_node = snarl.insert_node(pos, new_node);
//                         let dst_pin = OutPinId {
//                             node: new_node,
//                             output: 0,
//                         };

//                         // Connect the wire.
//                         for src_pin in pins {
//                             let src_ty =
//                                 pin_in_compat(snarl.get_node(src_pin.node).unwrap(), src_pin.input);
//                             if src_ty & dst_ty != 0 {
//                                 // In this demo, input pin MUST be unique ...
//                                 // Therefore here we drop inputs of source input pin.
//                                 snarl.drop_inputs(*src_pin);
//                                 snarl.connect(dst_pin, *src_pin);
//                                 ui.close_menu();
//                             }
//                         }
//                     }
//                 }
//             }
//         };
//     }

//     fn has_node_menu(&mut self, _node: &DemoNode) -> bool {
//         true
//     }

//     fn show_node_menu(
//         &mut self,
//         node: NodeId,
//         _inputs: &[InPin],
//         _outputs: &[OutPin],
//         ui: &mut Ui,
//         snarl: &mut Snarl<DemoNode>,
//     ) {
//         ui.label("Node menu");
//         if ui.button("Remove").clicked() {
//             snarl.remove_node(node);
//             ui.close_menu();
//         }
//     }

//     fn has_on_hover_popup(&mut self, _: &DemoNode) -> bool {
//         true
//     }

//     fn show_on_hover_popup(
//         &mut self,
//         node: NodeId,
//         _inputs: &[InPin],
//         _outputs: &[OutPin],
//         ui: &mut Ui,
//         snarl: &mut Snarl<DemoNode>,
//     ) {
//         match snarl[node] {
//             DemoNode::Sink => {
//                 ui.label("Displays anything connected to it");
//             }
//             DemoNode::Number(_) => {
//                 ui.label("Outputs integer value");
//             }
//             DemoNode::String(_) => {
//                 ui.label("Outputs string value");
//             }
//             DemoNode::ShowImage(_) => {
//                 ui.label("Displays image from URL in input");
//             }
//             DemoNode::ExprNode(_) => {
//                 ui.label("Evaluates algebraic expression with input for each unique variable name");
//             }
//         }
//     }

//     fn header_frame(
//         &mut self,
//         frame: egui::Frame,
//         node: NodeId,
//         _inputs: &[InPin],
//         _outputs: &[OutPin],
//         snarl: &Snarl<DemoNode>,
//     ) -> egui::Frame {
//         match snarl[node] {
//             DemoNode::Sink => frame.fill(egui::Color32::from_rgb(70, 70, 80)),
//             DemoNode::Number(_) => frame.fill(egui::Color32::from_rgb(70, 40, 40)),
//             DemoNode::String(_) => frame.fill(egui::Color32::from_rgb(40, 70, 40)),
//             DemoNode::ShowImage(_) => frame.fill(egui::Color32::from_rgb(40, 40, 70)),
//             DemoNode::ExprNode(_) => frame.fill(egui::Color32::from_rgb(70, 66, 40)),
//         }
//     }
// }

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
struct ExprNode {
    text: String,
    bindings: Vec<String>,
    values: Vec<f64>,
    expr: Expr,
}

impl ExprNode {
    fn new() -> Self {
        ExprNode {
            text: "0".to_string(),
            bindings: Vec::new(),
            values: Vec::new(),
            expr: Expr::Val(0.0),
        }
    }

    fn eval(&self) -> f64 {
        self.expr.eval(&self.bindings, &self.values)
    }
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize, Debug)]
enum UnOp {
    Pos,
    Neg,
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize, Debug)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
enum Expr {
    Var(String),
    Val(f64),
    UnOp {
        op: UnOp,
        expr: Box<Expr>,
    },
    BinOp {
        lhs: Box<Expr>,
        op: BinOp,
        rhs: Box<Expr>,
    },
}

impl Expr {
    fn eval(&self, bindings: &[String], args: &[f64]) -> f64 {
        let binding_index =
            |name: &str| bindings.iter().position(|binding| binding == name).unwrap();

        match self {
            Expr::Var(name) => args[binding_index(name)],
            Expr::Val(value) => *value,
            Expr::UnOp { op, expr } => match op {
                UnOp::Pos => expr.eval(bindings, args),
                UnOp::Neg => -expr.eval(bindings, args),
            },
            Expr::BinOp { lhs, op, rhs } => match op {
                BinOp::Add => lhs.eval(bindings, args) + rhs.eval(bindings, args),
                BinOp::Sub => lhs.eval(bindings, args) - rhs.eval(bindings, args),
                BinOp::Mul => lhs.eval(bindings, args) * rhs.eval(bindings, args),
                BinOp::Div => lhs.eval(bindings, args) / rhs.eval(bindings, args),
            },
        }
    }

    fn extend_bindings(&self, bindings: &mut Vec<String>) {
        match self {
            Expr::Var(name) => {
                if !bindings.contains(name) {
                    bindings.push(name.clone());
                }
            }
            Expr::Val(_) => {}
            Expr::UnOp { expr, .. } => {
                expr.extend_bindings(bindings);
            }
            Expr::BinOp { lhs, rhs, .. } => {
                lhs.extend_bindings(bindings);
                rhs.extend_bindings(bindings);
            }
        }
    }
}

impl syn::parse::Parse for UnOp {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let lookahead = input.lookahead1();
        if lookahead.peek(syn::Token![+]) {
            input.parse::<syn::Token![+]>()?;
            Ok(UnOp::Pos)
        } else if lookahead.peek(syn::Token![-]) {
            input.parse::<syn::Token![-]>()?;
            Ok(UnOp::Neg)
        } else {
            Err(lookahead.error())
        }
    }
}

impl syn::parse::Parse for BinOp {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let lookahead = input.lookahead1();
        if lookahead.peek(syn::Token![+]) {
            input.parse::<syn::Token![+]>()?;
            Ok(BinOp::Add)
        } else if lookahead.peek(syn::Token![-]) {
            input.parse::<syn::Token![-]>()?;
            Ok(BinOp::Sub)
        } else if lookahead.peek(syn::Token![*]) {
            input.parse::<syn::Token![*]>()?;
            Ok(BinOp::Mul)
        } else if lookahead.peek(syn::Token![/]) {
            input.parse::<syn::Token![/]>()?;
            Ok(BinOp::Div)
        } else {
            Err(lookahead.error())
        }
    }
}

impl syn::parse::Parse for Expr {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let lookahead = input.lookahead1();

        let lhs;
        if lookahead.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);
            let expr = content.parse::<Expr>()?;
            if input.is_empty() {
                return Ok(expr);
            }
            lhs = expr;
        // } else if lookahead.peek(syn::LitFloat) {
        //     let lit = input.parse::<syn::LitFloat>()?;
        //     let value = lit.base10_parse::<f64>()?;
        //     let expr = Expr::Val(value);
        //     if input.is_empty() {
        //         return Ok(expr);
        //     }
        //     lhs = expr;
        } else if lookahead.peek(syn::LitInt) {
            let lit = input.parse::<syn::LitInt>()?;
            let value = lit.base10_parse::<f64>()?;
            let expr = Expr::Val(value);
            if input.is_empty() {
                return Ok(expr);
            }
            lhs = expr;
        } else if lookahead.peek(syn::Ident) {
            let ident = input.parse::<syn::Ident>()?;
            let expr = Expr::Var(ident.to_string());
            if input.is_empty() {
                return Ok(expr);
            }
            lhs = expr;
        } else {
            let unop = input.parse::<UnOp>()?;

            return Self::parse_with_unop(unop, input);
        }

        let binop = input.parse::<BinOp>()?;

        Self::parse_binop(Box::new(lhs), binop, input)
    }
}

impl Expr {
    fn parse_with_unop(op: UnOp, input: syn::parse::ParseStream) -> syn::Result<Self> {
        let lookahead = input.lookahead1();

        let lhs;
        if lookahead.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);
            let expr = Expr::UnOp {
                op,
                expr: Box::new(content.parse::<Expr>()?),
            };
            if input.is_empty() {
                return Ok(expr);
            }
            lhs = expr;
        } else if lookahead.peek(syn::LitFloat) {
            let lit = input.parse::<syn::LitFloat>()?;
            let value = lit.base10_parse::<f64>()?;
            let expr = Expr::UnOp {
                op,
                expr: Box::new(Expr::Val(value)),
            };
            if input.is_empty() {
                return Ok(expr);
            }
            lhs = expr;
        } else if lookahead.peek(syn::LitInt) {
            let lit = input.parse::<syn::LitInt>()?;
            let value = lit.base10_parse::<f64>()?;
            let expr = Expr::UnOp {
                op,
                expr: Box::new(Expr::Val(value)),
            };
            if input.is_empty() {
                return Ok(expr);
            }
            lhs = expr;
        } else if lookahead.peek(syn::Ident) {
            let ident = input.parse::<syn::Ident>()?;
            let expr = Expr::UnOp {
                op,
                expr: Box::new(Expr::Var(ident.to_string())),
            };
            if input.is_empty() {
                return Ok(expr);
            }
            lhs = expr;
        } else {
            return Err(lookahead.error());
        }

        let op = input.parse::<BinOp>()?;

        Self::parse_binop(Box::new(lhs), op, input)
    }

    fn parse_binop(lhs: Box<Expr>, op: BinOp, input: syn::parse::ParseStream) -> syn::Result<Self> {
        let lookahead = input.lookahead1();

        let rhs;
        if lookahead.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);
            rhs = Box::new(content.parse::<Expr>()?);
            if input.is_empty() {
                return Ok(Expr::BinOp { lhs, op, rhs });
            }
        } else if lookahead.peek(syn::LitFloat) {
            let lit = input.parse::<syn::LitFloat>()?;
            let value = lit.base10_parse::<f64>()?;
            rhs = Box::new(Expr::Val(value));
            if input.is_empty() {
                return Ok(Expr::BinOp { lhs, op, rhs });
            }
        } else if lookahead.peek(syn::LitInt) {
            let lit = input.parse::<syn::LitInt>()?;
            let value = lit.base10_parse::<f64>()?;
            rhs = Box::new(Expr::Val(value));
            if input.is_empty() {
                return Ok(Expr::BinOp { lhs, op, rhs });
            }
        } else if lookahead.peek(syn::Ident) {
            let ident = input.parse::<syn::Ident>()?;
            rhs = Box::new(Expr::Var(ident.to_string()));
            if input.is_empty() {
                return Ok(Expr::BinOp { lhs, op, rhs });
            }
        } else {
            return Err(lookahead.error());
        }

        let next_op = input.parse::<BinOp>()?;

        if let (BinOp::Add | BinOp::Sub, BinOp::Mul | BinOp::Div) = (op, next_op) {
            let rhs = Self::parse_binop(rhs, next_op, input)?;
            Ok(Self::BinOp {
                lhs,
                op,
                rhs: Box::new(rhs),
            })
        } else {
            let lhs = Self::BinOp { lhs, op, rhs };
            Self::parse_binop(Box::new(lhs), next_op, input)
        }
    }
}

use std::sync::mpsc::Receiver;

pub struct DemoApp {
    shared_state: Arc<SharedState>,        // Shared state
    update_receiver: Option<Receiver<()>>, // Receiver for update signals
    stop_monitoring: watch::Sender<bool>,  // Signal to stop monitoring
}

impl DemoApp {
    pub fn new(cx: &CreationContext) -> Self {
        println!("Initializing DemoApp...");
        // Set the default theme to dark
        // Set the default theme to dark if no theme is set in the context.
        if cx.egui_ctx.style().visuals.dark_mode {
            cx.egui_ctx.set_visuals(egui::Visuals::dark());
        } else {
            cx.egui_ctx.set_visuals(egui::Visuals::light());
        }

        egui_extras::install_image_loaders(&cx.egui_ctx);

        cx.egui_ctx.style_mut(|style| style.animation_time *= 10.0);

        // Initialize shared state
        let shared_state = Arc::new(SharedState::default());
        // Retrieve the HWND after the GUI is initialized
        let hwnd = get_current_window_handle();
        if hwnd == 0 {
            eprintln!("Failed to retrieve HWND. Exiting...");
            log::error!("Failed to retrieve HWND. Exiting...");
            std::process::exit(1);
        }

        // Ensure single instance with the valid HWND
        if ensure_single_instance(hwnd).is_none() {
            println!("Another instance is already running. Exiting...");
            // Optionally, display a message box or keep the window open
            #[cfg(target_os = "windows")]
            unsafe {
                use std::ffi::CString;
                use winapi::um::winuser::{MB_ICONERROR, MB_OK, MessageBoxA};
                #[cfg(feature = "uses_funny")]
                let joke = jokes::get_next_joke();
                #[cfg(not(feature = "uses_funny"))]
                let joke = "";

                let message = CString::new(format!(
                    "Another instance is already running. Exiting...\n\n {}",
                    joke
                ))
                .unwrap();
                let title = CString::new("Application Error").unwrap();
                MessageBoxA(
                    std::ptr::null_mut(),
                    message.as_ptr(),
                    title.as_ptr(),
                    MB_OK | MB_ICONERROR,
                );
            }
            std::process::exit(1);
        }

        // Create a watch channel for stop monitoring
        let (stop_monitoring_tx, _) = watch::channel(false);

        // Clone the sender for the monitoring task
        let stop_monitoring_tx_clone = stop_monitoring_tx.clone();
        let chrome_shared_state = shared_state.clone();
        let ctx_clone = cx.egui_ctx.clone();
        tokio::spawn(async move {
            if let Err(err) = monitor_chrome_and_update_shared_state(
                chrome_shared_state,
                stop_monitoring_tx_clone.subscribe(),
                &ctx_clone,
            )
            .await
            {
                eprintln!("Error monitoring Chrome: {:?}", err);
            }
        });

        // Start the joke updater
        #[cfg(feature = "uses_funny")]
        jokes::start_joke_updater();

        // Start the Salvo server in a separate task
        let server_state = shared_state.clone();
        tokio::spawn(async move {
            start_server(server_state).await;
        });

        // Create a channel to signal updates to the egui app
        let (update_tx, update_rx) = std::sync::mpsc::channel();

        // Spawn a background task to poll for updates
        let polling_shared_state = shared_state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await; // Poll every 100ms
                let needs_update = {
                    let mut flag = polling_shared_state.needs_update.lock().await; // Use async lock
                    if *flag {
                        *flag = false; // Reset the flag
                        true
                    } else {
                        false
                    }
                };
                if needs_update {
                    update_tx.send(()).unwrap(); // Signal the egui app to update
                }
            }
        });

        DemoApp {
            shared_state,
            update_receiver: Some(update_rx),
            stop_monitoring: stop_monitoring_tx, // Use the original sender here
        }
    }

    fn graceful_exit(&mut self) {
        println!("Performing cleanup before exiting...");

        println!("Saving state...");
        // if let Some(storage) = storage {
        //     self.save(storage);
        // }
        // Send shutdown signal to the web server
        if let Err(err) = self.shared_state.shutdown_signal.send(true) {
            eprintln!("Failed to send shutdown signal: {:?}", err);
        }
        // Notify background tasks to stop
        if let Err(err) = self.stop_monitoring.send(true) {
            eprintln!("Failed to send stop signal to monitoring task: {:?}", err);
        }

        // Perform any additional cleanup here
        println!("Cleanup complete. Exiting application.");

        // Exit the application
        // std::process::exit(0);
    }
    pub fn set_update_receiver(&mut self, receiver: Receiver<()>) {
        self.update_receiver = Some(receiver);
    }
}

impl App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for update signals
        if let Some(receiver) = &self.update_receiver {
            while receiver.try_recv().is_ok() {
                ctx.request_repaint(); // Trigger a repaint
            }
        }

        // Check if the modal dialog should be displayed
        if self
            .shared_state
            .show_modal_disconnect
            .load(Ordering::SeqCst)
        {
            // Add a full-screen transparent grey overlay
            egui::Area::new("modal_overlay".into())
                .interactable(false) // Make the overlay non-interactive
                .fixed_pos(egui::Pos2::ZERO) // Start at the top-left corner
                .show(ctx, |ui| {
                    let screen_rect = ctx.screen_rect();
                    ui.allocate_ui_at_rect(screen_rect, |ui| {
                        ui.painter().rect_filled(
                            screen_rect,
                            0.0,
                            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 128), // Semi-transparent grey
                        );
                    });
                });

            // Center the modal dialog
            egui::Area::new("modal_dialog".into())
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO) // Center the modal
                .show(ctx, |ui| {
                    egui::Frame::window(&ctx.style())
                        .fill(egui::Color32::from_rgb(240, 240, 240)) // Modal background color
                        .rounding(10.0) // Rounded corners
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.heading("Chrome Disconnected");
                                ui.label("Chrome has exited. Do you want to clear the tabs?");
                                ui.separator();
                                ui.horizontal(|ui| {
                                    if ui.button("Clear Tabs").clicked() {
                                        self.shared_state.tabs.clear();
                                        println!("Tabs cleared");
                                        self.shared_state
                                            .show_modal_disconnect
                                            .store(false, Ordering::SeqCst); // Reset the flag
                                    }
                                    if ui.button("Dismiss").clicked() {
                                        println!("Modal dismissed");
                                        self.shared_state
                                            .show_modal_disconnect
                                            .store(false, Ordering::SeqCst); // Reset the flag
                                    }
                                    if ui.button("Exit").clicked() {
                                        self.graceful_exit(); // Call the graceful exit function
                                        let ctx_clone = ctx.clone();
                                        tokio::spawn(async move {
                                            tokio::time::sleep(std::time::Duration::from_secs(3))
                                                .await;
                                            ctx_clone
                                                .send_viewport_cmd(egui::ViewportCommand::Close);
                                        });
                                    }
                                });
                            });
                        });
                });
        }
        // Ensure the UI refreshes periodically
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        // let mut snarl = {
        //     let snarl = self.shared_state.snarl.try_lock(); // Use blocking_lock to avoid async context
        //     snarl.map(|s| s.clone()) // Clone the tabs to avoid holding the lock
        // };

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    ui.menu_button("Quit", |ui| {
                        // if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        // }
                    });
                    if ui.button("Launch Chrome").clicked() {
                        let stop_monitoring_tx = self.stop_monitoring.clone();
                        let shared_state = self.shared_state.clone();
                        let user_data_dir = std::env::temp_dir().join("chrome_user_data");
                        let ctx_clone = ctx.clone();
                        let notify = shared_state.monitoring_state.get_notify();

                        tokio::spawn(async move {
                            // Signal the current monitor to stop if active
                            if shared_state.monitoring_state.is_running() {
                                if let Err(e) = stop_monitoring_tx.send(true) {
                                    eprintln!("Failed to send stop signal: {:?}", e);
                                } else {
                                    println!("Signaled monitoring task to stop...");
                                }

                                // Wait for the monitoring task to stop using Notify
                                notify.notified().await;
                                println!("Verified monitoring task has stopped.");
                            } else {
                                println!("No active monitoring task to stop.");
                            }

                            // Launch Chrome
                            match launch_chrome(&user_data_dir, shared_state.clone()) {
                                Ok(_) => println!("Chrome launched successfully"),
                                Err(err) => {
                                    eprintln!("Failed to launch Chrome: {:?}", err);
                                    return;
                                }
                            }

                            // Wait for Chrome to initialize
                            let mut attempts = 0;
                            let max_attempts = 10;
                            let retry_delay = std::time::Duration::from_secs(2);
                            let mut connected = false;

                            while attempts < max_attempts {
                                match reqwest::get("http://localhost:9222/json").await {
                                    Ok(response) => {
                                        if response.status().is_success() {
                                            println!("Connected to Chrome DevTools Protocol.");
                                            connected = true;
                                            break;
                                        }
                                    }
                                    Err(err) => {
                                        eprintln!("Error connecting to Chrome: {:?}", err);
                                    }
                                }

                                attempts += 1;
                                println!(
                                    "Retrying connection in {} seconds...",
                                    retry_delay.as_secs()
                                );
                                tokio::time::sleep(retry_delay).await;
                            }

                            if !connected {
                                eprintln!(
                                    "Failed to connect to Chrome after {} attempts.",
                                    max_attempts
                                );
                                return;
                            }

                            // Reset the stop signal
                            if let Err(e) = stop_monitoring_tx.send(false) {
                                eprintln!("Failed to reset stop signal: {:?}", e);
                            }

                            // Start monitoring again
                            let stop_signal = stop_monitoring_tx.subscribe();
                            shared_state.monitoring_state.set_running(true); // Set monitoring as active
                            if let Err(err) = monitor_chrome_and_update_shared_state(
                                shared_state.clone(),
                                stop_signal,
                                &ctx_clone,
                            )
                            .await
                            {
                                eprintln!("Error monitoring Chrome: {:?}", err);
                            }
                            shared_state.monitoring_state.set_running(false); // Set monitoring as inactive
                        });
                    }
                    ui.menu_button("Monitor", |ui| {
                        // Display the current monitoring status
                        let is_running = self.shared_state.monitoring_state.is_running();
                        let is_connected = self.shared_state.monitoring_state.is_connected();

                        ui.label(format!(
                            "Monitoring: {}",
                            if is_running { "Running" } else { "Stopped" }
                        ));
                        ui.label(format!(
                            "Connected: {}",
                            if is_connected { "Yes" } else { "No" }
                        ));

                        ui.separator();

                        // Button to start monitoring
                        if ui.button("Start Monitoring").clicked() {
                            if !is_running {
                                let stop_monitoring_tx = self.stop_monitoring.clone();
                                let shared_state = self.shared_state.clone();
                                let ctx_clone = ctx.clone();

                                tokio::spawn(async move {
                                    shared_state.monitoring_state.set_running(true);
                                    if let Err(err) = monitor_chrome_and_update_shared_state(
                                        shared_state,
                                        stop_monitoring_tx.subscribe(),
                                        &ctx_clone,
                                    )
                                    .await
                                    {
                                        eprintln!("Error starting monitoring: {:?}", err);
                                    }
                                });
                            }
                        }

                        // Button to stop monitoring
                        if ui.button("Stop Monitoring").clicked() {
                            if is_running {
                                self.shared_state.monitoring_state.set_running(false);
                                if let Err(err) = self.stop_monitoring.send(true) {
                                    eprintln!("Failed to send stop signal: {:?}", err);
                                }
                            }
                        }
                    });
                    ui.add_space(16.0);
                    egui::widgets::global_theme_preference_switch(ui);
                    // Add a spacer to push the joke label to the right
                    #[cfg(feature = "uses_funny")]
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(jokes::get_curr_joke());
                    });
                }

                egui::widgets::global_theme_preference_switch(ui);

                if ui.button("Clear All").clicked() {
                    // if let Some(global_snarl) = GLOBAL_SNARL.get() {
                    //     let mut snarl = global_snarl.lock().unwrap();
                    //     snarl.clear(); // Clear all nodes in the global Snarl
                    // }
                }
            });
        });

        // Left panel for graph style settings and adding a new tab
        // egui::SidePanel::left("left_panel").show(ctx, |ui| {
        //     ui.heading("Graph Style");
        //     ui.separator();

        //     ui.label("Node Layout:");
        //     egui::ComboBox::from_label("Layout")
        //         .selected_text(format!("{:?}", self.style.node_layout.unwrap_or_default()))
        //         .show_ui(ui, |ui| {
        //             ui.selectable_value(
        //                 &mut self.style.node_layout,
        //                 Some(NodeLayout::FlippedSandwich),
        //                 "Flipped Sandwich",
        //             );
        //             ui.selectable_value(
        //                 &mut self.style.node_layout,
        //                 Some(NodeLayout::Basic),
        //                 "Horizontal",
        //             );
        //         });

        //     ui.label("Pin Placement:");
        //     egui::ComboBox::from_label("Placement")
        //         .selected_text(format!(
        //             "{:?}",
        //             self.style.pin_placement.unwrap_or_default()
        //         ))
        //         .show_ui(ui, |ui| {
        //             ui.selectable_value(
        //                 &mut self.style.pin_placement,
        //                 Some(PinPlacement::Edge),
        //                 "Edge",
        //             );
        //             ui.selectable_value(
        //                 &mut self.style.pin_placement,
        //                 Some(PinPlacement::Center),
        //                 "Center",
        //             );
        //         });

        //     // Pin size slider
        //     let mut pin_size = self.style.pin_size.unwrap_or(7.0);
        //     if ui
        //         .add(egui::Slider::new(&mut pin_size, 5.0..=15.0).text("Pin Size"))
        //         .changed()
        //     {
        //         self.style.pin_size = Some(pin_size); // Update the pin size in the style
        //         ctx.request_repaint(); // Trigger a repaint to apply the changes
        //     }

        //     ui.separator();
        //     ui.heading("Actions");
        //     // Button to create a new tab
        //     // if ui.button("Create New Tab").clicked() {
        //     //     let new_tab = Tab {
        //     //         target_id: "new_target".to_string(),
        //     //         url: "https://example.com".to_string(),
        //     //         bang_id: "new_bang".to_string(),
        //     //         title: "New Tab".to_string(),
        //     //     };

        //     //     // Perform the POST request in a background task
        //     //     let client = reqwest::Client::new();
        //     //     tokio::spawn(async move {
        //     //         let response = client
        //     //             .post("http://127.0.0.1:5800/tabs")
        //     //             .json(&vec![new_tab]) // Send the new tab as JSON
        //     //             .send()
        //     //             .await;

        //     //         match response {
        //     //             Ok(res) => println!("Tab created successfully: {:?}", res.status()),
        //     //             Err(err) => eprintln!("Failed to create tab: {:?}", err),
        //     //         }
        //     //     });
        //     // }

        //     // if ui.button("Add Node").clicked() {
        //     //     // let node_id = NodeId(0); // Manually construct a new NodeId with a value
        //     //     // self.shared_state.snarl.insert(
        //     //     //     node_id,
        //     //     //     DemoNode::Number(0.0), // Add a new node of type `Number`
        //     //     // );
        //     //     // println!("Node added to Snarl graph: {:?}", node_id);
        //     // }
        // });

        // Right panel for port number, messages, and tabs
        let shared_port = {
            self.shared_state.port.try_lock() // Use blocking_lock to avoid async context
        };
        egui::SidePanel::right("right_panel")
            .resizable(true) // Allow resizing
            .default_width(200.0) // Set a default width
            .min_width(150.0)
            .show(ctx, |ui| {
                ui.heading("Server Settings");
                ui.separator();

                // Port number input
                let port = 5800; // Default port
                // port = *shared_port;
                // if ui.add(egui::DragValue::new(&mut port).clamp_range(1024..=65535)).changed() {
                //     *shared_port = port;
                //     println!("Port updated to: {}", port);
                // }
                ui.label(format!("Current Port: {}", port));

                ui.separator();
                ui.heading("Messages:");
                for message in self.shared_state.messages.iter() {
                    ui.label(message.value()); // Display each message individually
                }

                ui.separator();
                ui.heading("Tabs:");
                for mut tab in self.shared_state.tabs.iter_mut() {
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut tab.value_mut().target_id);
                        ui.text_edit_singleline(&mut tab.value_mut().url);
                        ui.text_edit_singleline(&mut tab.value_mut().bang_id);
                    });
                }
                // let mut tabs_to_remove = Vec::new();

                for tab in self.shared_state.tabs.iter() {
                    ui.horizontal(|ui| {
                        // Add a small close button
                        if ui.button("Close").clicked() {
                            // tabs_to_remove.push(tab.key().clone());
                            let target_id = tab.target_id.clone();
                            let url = tab.url.clone();
                            let ws_stream = self.shared_state.clone(); // Clone shared state for async task
                            tokio::spawn(async move {
                                if let Err(err) = close_target(&target_id, &url).await {
                                    eprintln!("Failed to close target {}: {:?}", target_id, err);
                                } else {
                                    println!("Target {} closed successfully", target_id);
                                }
                            });
                        }
                        ui.label(&tab.value().target_id);
                        ui.label(&tab.value().url);
                        ui.label(&tab.value().bang_id);
                    });
                }

                // for target_id in tabs_to_remove {
                //     self.shared_state.tabs.remove(&target_id);
                // }
            });
        // Bottom panel for JSON view
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.heading("debugchrome:/ console");
            // let snarl_json = serde_json::to_string(
            // &self.shared_state.snarl.iter().map(|entry| (entry.key().clone(), entry.value().clone())).collect::<Vec<_>>()
            // ).unwrap();
            // ui.label("Snarl Graph as JSON:");
            // ui.label(snarl_json);
        });
        // Central panel for the Snarl graph
        egui::CentralPanel::default().show(ctx, |ui| {
            // Lock the shared Snarl graph

            // // Display the nodes in the Snarl graph
            // let snarl = self.shared_state.snarl.lock().unwrap();
            // ui.label("Nodes:");
            // let snarl_json = serde_json::to_string(&*snarl).unwrap();
            // ui.label("Snarl Graph as JSON:");
            // ui.label(snarl_json);

            // // Display the messages
            // let messages = self.shared_state.messages.lock().unwrap();
            // ui.label("Messages:");
            // for message in messages.iter() {
            //     ui.label(message);
            // }            let mut snarl = self.shared_state.snarl.lock().unwrap();

            // Render the Snarl graph
            ui.heading("debugchrome: console");
            // Add a hyperlink to the Salvo server URL
            // ui.horizontal(|ui| {
            //     ui.label("Open Salvo server:");
            //     ui.add(egui::Hyperlink::from_label_and_url(
            //         "debugchrome://http://127.0.0.1:5800",
            //         "debugchrome://http://127.0.0.1:5800",
            //     ));
            // });
            // Temporarily modify the visuals to make the button look like a hyperlink

            // Temporarily modify the visuals to make the button look like a hyperlink
            // Temporarily modify the visuals to make the button look like a hyperlink
            {
                let visuals = ui.visuals_mut();
                visuals.widgets.inactive.bg_fill = egui::Color32::TRANSPARENT; // Remove button background
                visuals.widgets.inactive.fg_stroke.color = egui::Color32::from_rgb(0, 122, 204); // Hyperlink blue
            }

            if ui.button("debugchrome://http://127.0.0.1:5800").clicked() {
                // Launch debugchrome programmatically
                let url = "debugchrome://http://127.0.0.1:5800";
                if let Err(err) = std::process::Command::new("debugchrome.exe")
                    .arg(url)
                    .spawn()
                {
                    eprintln!("Failed to launch DebugChrome: {}", err);
                }
            }

            // Restore the original visuals
            {
                let visuals = ui.visuals_mut();
                visuals.widgets.inactive.bg_fill = egui::Color32::from_gray(30); // Restore original background
                visuals.widgets.inactive.fg_stroke.color = egui::Color32::from_gray(200); // Restore original text color
            }
            // Lock the shared Snarl graph

            // // Display the messages
            // let messages = self.shared_state.messages.lock().unwrap();
            // ui.label("Messages:");
            // for message in messages.iter() {
            //     ui.label(message);
            // }            let mut snarl = self.shared_state.snarl.lock().unwrap();

            // Render the Snarl graph
            // Render the Snarl graph using DashMap

            // // Display the nodes in the Snarl graph
            // ui.label("Nodes:");
            // let snarl_json = serde_json::to_string(&*snarl).unwrap();
            // ui.label("Snarl Graph as JSON:");
            // ui.label(snarl_json);
            // SnarlWidget::new()
            //     .id(Id::new("snarl-demo"))
            //     .style(self.style.clone())
            //     .show(&mut *snarl, &mut DemoViewer, ui);
            // SnarlWidget::new()
            //     .id(Id::new("snarl-demo"))
            //     .style(self.style.clone())
            //     .show(&mut snarl, &mut DemoViewer, ui);
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        println!("Saving state...");
    }
}

// When compiling natively:
//#[cfg(not(target_arch = "wasm32"))]

//#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 300.0])
            .with_min_inner_size([300.0, 220.0]),
        ..Default::default()
    };

    eframe::run_native(
        "egui-snarl demo",
        native_options,
        Box::new(|cx| Ok(Box::new(DemoApp::new(cx)))),
    )
}

#[cfg(target_arch = "wasm32")]
fn get_canvas_element() -> Option<web_sys::HtmlCanvasElement> {
    use eframe::wasm_bindgen::JsCast;

    let document = web_sys::window()?.document()?;
    let canvas = document.get_element_by_id("egui_snarl_demo")?;
    canvas.dyn_into::<web_sys::HtmlCanvasElement>().ok()
}

// When compiling to web using trunk:
#[cfg(target_arch = "wasm32")]
fn main() {
    let canvas = get_canvas_element().expect("Failed to find canvas with id 'egui_snarl_demo'");

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cx| Ok(Box::new(DemoApp::new(cx)))),
            )
            .await
            .expect("failed to start eframe");
    });
}

fn format_float(v: f64) -> String {
    let v = (v * 1000.0).round() / 1000.0;
    format!("{v}")
}

// use egui_snarl::Snarl;
// use std::sync::{Arc, Mutex};

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct Tab {
    pub target_id: String,
    pub url: String,
    pub title: String,
    pub bang_id: String,
    pub browser_context_id: Option<String>,
}

// Shared state structure
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

#[derive(Debug)]
pub struct SharedState {
    pub messages: DashMap<usize, String>, // Use DashMap for messages
    pub port: tokio::sync::Mutex<u16>,    // Use DashMap for port
    pub tabs: DashMap<String, Tab>,       // Already using DashMap for tabs
    pub browser_hwnds: DashMap<String, isize>, // Map browserContextId or targetId to HWND
    pub needs_update: tokio::sync::Mutex<bool>, // Use DashMap for update flags
    pub show_modal_disconnect: AtomicBool, // Atomic flag for modal dialog
    pub shutdown_signal: watch::Sender<bool>, // Add shutdown signal
    pub monitoring_state: MonitoringState,
}

impl Default for SharedState {
    fn default() -> Self {
        let (shutdown_signal, _) = watch::channel(false); // Initialize shutdown signal
        SharedState {
            messages: DashMap::new(),
            port: tokio::sync::Mutex::new(5800),
            tabs: DashMap::new(),
            browser_hwnds: DashMap::new(), // Initialize the HWND map
            needs_update: tokio::sync::Mutex::new(false),
            show_modal_disconnect: AtomicBool::new(false),
            shutdown_signal,
            monitoring_state: MonitoringState::new(),
        }
    }
}

// Handler to log a message and return the current state
#[handler]
async fn hello_handler(depot: &mut Depot) -> String {
    let state = depot.obtain::<Arc<SharedState>>().unwrap();
    let message_id = state.messages.len(); // Use the current length as the message ID
    state
        .messages
        .insert(message_id, "Hello from Salvo!".to_string());
    let messages: Vec<_> = state
        .messages
        .iter()
        .map(|entry| entry.value().clone())
        .collect();
    format!("Hello, Salvo!\nMessages: {messages:#?}")
}

// Handler to get the list of tabs
#[handler]
async fn get_tabs_handler(depot: &mut Depot) -> String {
    let state = depot.obtain::<Arc<SharedState>>().unwrap();
    let tabs: Vec<_> = state
        .tabs
        .iter()
        .map(|entry| entry.value().clone())
        .collect();
    serde_json::to_string(&tabs).unwrap()
}

// Handler to update the list of tabs
#[handler]
async fn update_tabs_handler(req: &mut Request, depot: &mut Depot) -> String {
    let state = depot.obtain::<Arc<SharedState>>().unwrap();
    let new_tabs: Vec<Tab> = req.parse_json().await.expect("Failed to parse tabs");

    // Merge new tabs with existing ones, preserving unique target_ids
    for new_tab in new_tabs {
        state.tabs.insert(new_tab.target_id.clone(), new_tab);
    }

    // Signal that an update is needed
    let mut needs_update = state.needs_update.lock().await;
    *needs_update = true;

    "Tabs updated!".to_string()
}

// Function to start the Salvo server
pub async fn start_server(shared_state: Arc<SharedState>) {
    let mut shutdown_signal = shared_state.shutdown_signal.subscribe(); // Subscribe to the shutdown signal

    let router = Router::new()
        .hoop(affix_state::inject(shared_state)) // Inject shared state
        .get(hello_handler)
        .push(Router::with_path("tabs").get(get_tabs_handler))
        .push(Router::with_path("tabs").post(update_tabs_handler))
        .push(Router::with_path("hello").get(hello_handler));

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    println!("Salvo server running at http://127.0.0.1:5800");

    let server = Server::new(acceptor);

    tokio::select! {
        _ = server.serve(router) => {
            println!("Server stopped.");
        }
        _ = shutdown_signal.changed() => {
            println!("Shutdown signal received. Stopping server...");
        }
    }
}

// use chromiumoxide::browser::Browser;
// use chromiumoxide::cdp::browser_protocol::target::{TargetInfo, TargetId};
// use futures::StreamExt;
// use serde::Serialize;
// use std::sync::{Arc, Mutex};
// use tokio::task;
// use tokio_tungstenite::connect_async;
// use tokio_tungstenite::tungstenite::Message;

// #[derive(Debug, Serialize)]
// pub struct Tab {
//     pub target_id: String,
//     pub url: String,
//     pub bang_id: String,
// }

// #[derive(Default, Debug)]
// pub struct SharedState {
//     pub tabs: Mutex<Vec<Tab>>,
// }

// pub async fn monitor_chrome_and_update_shared_state(shared_state: Arc<SharedState>) -> anyhow::Result<()> {
//     // Connect to Chrome's DevTools Protocol
//     let (browser, mut handler): (Browser, Handler) =
//         Browser::connect("http://127.0.0.1:9222").await?;

//     // Enable target discovery
//     browser
//         .execute(TargetId::set_discover_targets(true))
//         .await?;

//     // Spawn a task to drive the DevTools connection in the background
//     task::spawn(async move {
//         while let Some(event) = handler.next().await {
//             println!("Received CDP event: {:?}", event);
//         }
//     });

//     // Fetch the initial list of tabs and update the shared state
//     let targets = browser.execute(TargetId::get_targets()).await?;
//     {
//         let mut tabs = shared_state.tabs.lock().unwrap();
//         tabs.clear();
//         for target in targets.target_infos {
//             if let Some(url) = target.url.clone() {
//                 tabs.push(Tab {
//                     target_id: target.target_id.clone(),
//                     url,
//                     bang_id: String::new(),
//                 });
//             }
//         }
//     }

//     // Listen for target events
//     let mut events = browser.event_listener::<TargetId>();
//     while let Some(event) = events.next().await {
//         match event {
//             TargetId::TargetDestroyed(params) => {
//                 println!("Target destroyed: {:?}", params.target_id);

//                 // // Notify the Salvo server via WebSocket
//                 // notify_salvo(&params.target_id).await?;

//                 // Remove the tab from the shared state
//                 let mut tabs = shared_state.tabs.lock().unwrap();
//                 tabs.retain(|tab| tab.target_id != params.target_id);
//             }
//             TargetId::TargetCreated(params) => {
//                 println!("Target created: {:?}", params.target_info);

//                 // Add the new tab to the shared state
//                 if let Some(url) = params.target_info.url.clone() {
//                     let mut tabs = shared_state.tabs.lock().unwrap();
//                     tabs.push(Tab {
//                         target_id: params.target_info.target_id.clone(),
//                         url,
//                         bang_id: String::new(),
//                     });
//                 }
//             }
//             _ => {}
//         }
//     }

//     Ok(())
// }

// async fn notify_salvo(target_id: &str) -> anyhow::Result<()> {
//     let (mut ws, _) = connect_async("ws://127.0.0.1:7878/ws").await?;
//     ws.send(Message::Text(format!(r#"{{"closed_target":"{}"}}"#, target_id)))
//         .await?;
//     Ok(())
// }

pub async fn monitor_chrome_and_update_shared_state(
    shared_state: Arc<SharedState>,
    stop_signal: watch::Receiver<bool>,
    ctx: &egui::Context,
) -> Result<(), Box<dyn std::error::Error>> {
    let retry_delay = std::time::Duration::from_secs(2);

    loop {
        println!("Attempting to connect to Chrome DevTools Protocol...");
        match reqwest::get("http://localhost:9222/json").await {
            Ok(response) => {
                let tabs_json: Vec<serde_json::Value> = response.json().await?;
                if let Some(url) = tabs_json
                    .get(0)
                    .and_then(|tab| tab.get("webSocketDebuggerUrl"))
                    .and_then(|url| url.as_str())
                {
                    println!("Successfully connected to Chrome DevTools Protocol.");
                    return monitor_chrome(shared_state.clone(), url.to_string(), stop_signal, ctx)
                        .await;
                } else {
                    println!("Failed to fetch WebSocket URL from Chrome DevTools.");
                }
            }
            Err(err) => {
                eprintln!("Error connecting to Chrome: {:?}", err);
            }
        }
        if *stop_signal.borrow() {
            println!("Monitoring stopped before connection was established.");
            shared_state.monitoring_state.notify.notify_waiters(); // Notify that the task has stopped
            return Ok(());
        }
        println!(
            "Retrying connection in {} seconds...",
            retry_delay.as_secs()
        );
        tokio::time::sleep(retry_delay).await;
    }
}

async fn monitor_chrome(
    shared_state: Arc<SharedState>,
    browser_ws_url: String,
    stop_signal: watch::Receiver<bool>,
    ctx: &egui::Context,
) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the shared state with the current tabs
    shared_state.tabs.clear();
    let response = reqwest::get("http://localhost:9222/json").await?;
    let tabs_json: Vec<serde_json::Value> = response.json().await?;
    for tab in &tabs_json {
        if let Some(_target_id) = tab.get("id").and_then(|id| id.as_str()) {
            if let Some(_url) = tab.get("url").and_then(|u| u.as_str()) {
                let title = tab
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                // shared_state.tabs.insert(
                //     target_id.to_string(),
                //     Tab {
                //         target_id: target_id.to_string(),
                //         url: url.to_string(),
                //         bang_id: String::new(),
                //         title: title1
                //     },
                // );
            }
        }
    }

    // Connect to the WebSocket URL
    println!("Connecting to Chrome WebSocket: {}", browser_ws_url);
    let (mut ws_stream, _) = connect_async(&browser_ws_url).await?;
    println!("Connected to Chrome WebSocket: {}", browser_ws_url);
    #[cfg(target_os = "windows")]
    println!(
        "\n\n\nhwnd {:?}\n\n\n",
        crate::find_chrome_with_debug_port()
    );
    // Enable target discovery
    let enable_discovery = json!({
        "id": 1,
        "method": "Target.setDiscoverTargets",
        "params": { "discover": true }
    });
    ws_stream
        .send(Message::Text(enable_discovery.to_string().into()))
        .await?;

    // Create a channel to handle incoming messages
    let (tx, mut rx) = mpsc::unbounded_channel();

    // Clone `shared_state` for the WebSocket task
    let shared_state_ws = shared_state.clone();
    let ws_stream = Arc::new(tokio::sync::Mutex::new(ws_stream));
    let ws_stream_clone = ws_stream.clone();
    let ctx_clone = ctx.clone();
    tokio::spawn(async move {
        let mut ws_stream = ws_stream_clone.lock().await;
        while let Some(msg) = ws_stream.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    println!("Received WebSocket message: {}", &text);
                    process_cdp(
                        &shared_state_ws,
                        text.to_owned(),
                        &ctx_clone,
                        &mut ws_stream,
                    )
                    .await;
                    if let Err(err) = tx.send(text) {
                        eprintln!("Failed to send message to channel: {:?}", err);
                    }
                }
                Ok(_) => {}
                Err(err) => {
                    eprintln!("WebSocket error: {:?}", err);
                    if let tokio_tungstenite::tungstenite::Error::Protocol(protocol_error) = &err {
                        if matches!(
                            protocol_error,
                            tokio_tungstenite::tungstenite::error::ProtocolError::ResetWithoutClosingHandshake
                        ) {
                            println!("\n\n\nHandling WebSocket Protocol(ResetWithoutClosingHandshake) error\n\n\n");
                            //shared_state_ws.show_modal_disconnect.store(true, Ordering::SeqCst);
                        }
                    }
                    break;
                }
            }
        }
    });

    // Process incoming messages and listen for stop signal
    while let Some(message) = rx.recv().await {
        if *stop_signal.borrow() {
            println!("Stopping Chrome monitoring...");
            break;
        }
        process_cdp(&shared_state, message, &ctx, &mut ws_stream.lock().await).await;
    }

    Ok(())
}

async fn process_cdp(
    shared_state: &Arc<SharedState>,
    message: tokio_tungstenite::tungstenite::Utf8Bytes,
    ctx: &egui::Context,
    ws_stream: &mut tokio::sync::MutexGuard<
        '_,
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
) {
    println!("Processing CDP message: {}", &message);

    if let Ok(event) = serde_json::from_str::<serde_json::Value>(&message) {
        println!("Parsed event: {:?}", event);

        if let Some(method) = event.get("method").and_then(|m| m.as_str()) {
            println!("Detected method: {}", method);

            match method {
                "Target.detachedFromTarget" => {
                    println!("Handling Target.detachedFromTarget event");

                    if let Some(params) = event.get("params").cloned() {
                        if let Some(browser_context_id) = params
                            .get("browserContextId")
                            .and_then(|b| b.as_str())
                            .map(String::from)
                        {
                            println!("BrowserContextId: {}", browser_context_id);

                            // Remove all tabs associated with this browserContextId
                            let tabs_to_remove: Vec<String> = shared_state
                                .tabs
                                .iter()
                                .filter_map(|entry| {
                                    if entry.value().browser_context_id.as_ref()
                                        == Some(&browser_context_id)
                                    {
                                        Some(entry.key().clone())
                                    } else {
                                        None
                                    }
                                })
                                .collect();

                            for target_id in tabs_to_remove {
                                shared_state.tabs.remove(&target_id);
                                println!("Removed tab with Target ID: {}", target_id);
                            }
                        }
                    }
                }
                "Inspector.detached" => {
                    println!("unHandling Inspector.detached event");
                    // Check if the modal is already shown
                    if shared_state.show_modal_disconnect.load(Ordering::SeqCst) {
                        println!("Disconnect modal already shown. Ignoring subsequent events.");
                        return;
                    }
                    let response = reqwest::get("http://localhost:9222/json").await;
                    if response.is_err() || !response.unwrap().status().is_success() {
                        println!("Chrome DevTools server is down. Setting disconnect modal.");
                        shared_state
                            .show_modal_disconnect
                            .store(true, Ordering::SeqCst);
                    }
                    //shared_state.show_modal_disconnect.store(true, Ordering::SeqCst);
                }
                "Target.targetCreated" => {
                    println!("Handling Target.targetCreated event");

                    if let Some(params) = event.get("params").cloned() {
                        println!("Params: {:?}", params);

                        if let Some(target_info) = params.get("targetInfo") {
                            if let Some(target_id) = target_info
                                .get("targetId")
                                .and_then(|t| t.as_str())
                                .map(String::from)
                            {
                                println!("Target ID: {}", target_id);

                                let url = target_info
                                    .get("url")
                                    .and_then(|u| u.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                println!("URL: {}", url);
                                let title = target_info
                                    .get("title")
                                    .and_then(|u| u.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                println!("title: {}", url);

                                println!(
                                    "Target ID: {} {:?}",
                                    target_id,
                                    target_info
                                        .get("type")
                                        .and_then(|t| t.as_str())
                                        .unwrap_or("")
                                );
                                if target_info.get("type").and_then(|t| t.as_str()) != Some("page")
                                {
                                    println!("Skipping target of type other than 'page'");
                                    return;
                                }
                                if target_info.get("attached").and_then(|a| a.as_bool())
                                    == Some(false)
                                {
                                    println!("Skipping target with 'attached' set to false");
                                    return;
                                }
                                let browser_context_id = target_info
                                    .get("browserContextId")
                                    .and_then(|b| b.as_str())
                                    .map(String::from);
                                shared_state.tabs.insert(
                                    target_id.clone(),
                                    Tab {
                                        target_id,
                                        url,
                                        bang_id: String::new(),
                                        title: title, // Initialize with an empty title
                                        browser_context_id,
                                    },
                                );
                                println!("Target created and added to shared state");
                            } else {
                                println!("No targetId found in targetInfo");
                            }
                        } else {
                            println!("No targetInfo found in params");
                        }
                    } else {
                        println!("No params found in event");
                    }
                }
                "Target.targetDestroyed" => {
                    println!("Handling Target.targetDestroyed event");

                    if let Some(params) = event.get("params").cloned() {
                        println!("Params: {:?}", params);

                        if let Some(target_id) = params
                            .get("targetId")
                            .and_then(|t| t.as_str())
                            .map(String::from)
                        {
                            println!("Target ID: {}", target_id);

                            if shared_state.tabs.contains_key(&target_id) {
                                println!("Target ID found in shared state, removing it");
                                shared_state.tabs.remove(&target_id);
                                println!("Target destroyed and removed from shared state");
                            } else {
                                println!("Target ID not found in shared state, nothing to remove");
                            }
                        } else {
                            println!("No targetId found in params");
                        }
                    } else {
                        println!("No params found in event");
                    }
                }
                "Target.targetInfoChanged" => {
                    println!("Handling Target.targetInfoChanged event");

                    if let Some(params) = event.get("params").cloned() {
                        if let Some(target_info) = params.get("targetInfo") {
                            if let Some(target_id) = target_info
                                .get("targetId")
                                .and_then(|t| t.as_str())
                                .map(String::from)
                            {
                                println!(
                                    "Target ID: {} {:?}",
                                    target_id,
                                    target_info
                                        .get("type")
                                        .and_then(|t| t.as_str())
                                        .unwrap_or("")
                                );
                                if target_info.get("type").and_then(|t| t.as_str()) != Some("page")
                                {
                                    println!("Skipping target of type other than 'page'");
                                    return;
                                }
                                if target_info.get("attached").and_then(|a| a.as_bool())
                                    == Some(false)
                                {
                                    println!("Skipping target with 'attached' set to false");
                                    return;
                                }

                                let url = target_info
                                    .get("url")
                                    .and_then(|u| u.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let title = match query_target_title(ws_stream, &target_id).await {
                                    Ok(queried_title) => queried_title,
                                    Err(_) => target_info
                                        .get("title")
                                        .and_then(|t| t.as_str())
                                        .unwrap_or("Unknown Title")
                                        .to_string(),
                                };

                                println!("Target ID: {}", target_id);
                                println!("Updated URL: {}", url);
                                println!("Queried Title: {}", title);
                                let browser_context_id = target_info
                                    .get("browserContextId")
                                    .and_then(|b| b.as_str())
                                    .map(String::from);
                                // Update the tab in shared_state.tabs
                                if let Some(mut tab) = shared_state.tabs.get_mut(&target_id) {
                                    tab.url = url;
                                    tab.bang_id = title;
                                    println!("Target info updated in shared state");
                                } else {
                                    println!(
                                        "Target ID not found in shared state, adding new entry"
                                    );
                                    shared_state.tabs.insert(
                                        target_id.clone(),
                                        Tab {
                                            target_id,
                                            url,
                                            bang_id: String::new(),
                                            title: title.clone(), // Initialize with the queried title
                                            browser_context_id,
                                        },
                                    );
                                }

                                // Request a repaint to update the UI
                                ctx.request_repaint();
                            } else {
                                println!("No targetId found in targetInfo");
                            }
                        } else {
                            println!("No targetInfo found in params");
                        }
                    } else {
                        println!("No params found in event");
                    }
                }
                _ => {
                    println!("Unhandled method: {}", method);
                }
            }
        } else {
            println!("No method found in event");
        }
    } else {
        println!("Failed to parse message as JSON");
    }
}

use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

#[cfg(target_os = "windows")]
use winreg::RegKey;
#[cfg(target_os = "windows")]
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};
#[cfg(target_os = "windows")]
fn find_chrome_via_registry() -> Option<String> {
    let hk = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\chrome.exe",
            KEY_READ | KEY_WOW64_64KEY,
        )
        .ok()?;
    hk.get_value("").ok()
}

fn launch_chrome(user_data_dir: &Path, shared_state: Arc<SharedState>) -> io::Result<()> {
    // Find the real path to the Chrome executable
    #[cfg(target_os = "windows")]
    let chrome_path = find_chrome_via_registry()
        .map(PathBuf::from)
        .or_else(|| which("chrome.exe").ok());

    #[cfg(not(target_os = "windows"))]
    let chrome_path = which("chrome").ok();

    #[cfg(target_os = "windows")]
    let process = if let Some(chrome_path) = chrome_path {
        println!("Found Chrome executable at: {}", chrome_path.display());

        Command::new(chrome_path)
            .args([
                "--remote-debugging-port=9222",
                "--enable-automation",
                "--no-first-run",
                &format!("--user-data-dir={}", user_data_dir.display()),
            ])
            .spawn()?
    } else {
        eprintln!(
            "Warning: Chrome executable not found in registry or PATH using start (no HWND lookup)"
        );
        Command::new("cmd")
            .args([
                "/C",
                "start",
                "chrome",
                "--remote-debugging-port=9222",
                "--enable-automation",
                "--no-first-run",
                &format!("--user-data-dir={}", user_data_dir.display()),
            ])
            .spawn()?
    };

    #[cfg(not(target_os = "windows"))]
    let process = if let Some(chrome_path) = chrome_path {
        Command::new(chrome_path)
            .args([
                "--remote-debugging-port=9222",
                "--enable-automation",
                "--no-first-run",
                &format!("--user-data-dir={}", user_data_dir.display()),
            ])
            .spawn()?
    } else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Chrome executable not found",
        ));
    };

    let polling_shared_state = shared_state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let hwnd = wait_for_browser_hwnd(process.id()).ok().unwrap();
        println!("Retrieved HWND for browser: {}", hwnd);
        polling_shared_state
            .browser_hwnds
            .insert(process.id().to_string(), hwnd);
    });
    // Wait for the browser window to appear and retrieve its HWND

    // Store the HWND in the browser_hwnds map using the PID as the temporary key
    //shared_state.browser_hwnds.insert(process.id().to_string(), hwnd);
    Ok(())
}

async fn query_target_title(
    ws_stream: &mut tokio::sync::MutexGuard<
        '_,
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    target_id: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    // Attach to the target
    let attach_message = json!({
        "id": 1,
        "method": "Target.attachToTarget",
        "params": {
            "targetId": target_id,
            "flatten": true
        }
    });
    ws_stream
        .send(Message::Text(attach_message.to_string().into()))
        .await?;

    // Execute JavaScript to get the title
    let eval_message = json!({
        "id": 2,
        "method": "Runtime.evaluate",
        "params": {
            "expression": "document.title",
            "returnByValue": true
        }
    });
    ws_stream
        .send(Message::Text(eval_message.to_string().into()))
        .await?;

    // Wait for the response
    while let Some(msg) = ws_stream.next().await {
        if let Ok(Message::Text(response)) = msg {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&response) {
                if json.get("id") == Some(&serde_json::Value::from(2)) {
                    return json
                        .get("result")
                        .and_then(|result| result.get("value"))
                        .and_then(|value| value.as_str())
                        .map(String::from)
                        .ok_or_else(|| "Failed to extract title from JSON response".into());
                } else if json.get("error").is_some() {
                    return Err("Error response received from Chrome DevTools".into());
                }
            }
        }
    }

    Ok("Failed to get target title".to_string())
}

#[cfg(feature = "uses_gui")]
pub async fn start_gui() -> eframe::Result<()> {
    use chrono::{NaiveDate, Utc};

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_app_id("debugchrome"),
        ..Default::default()
    };

    const VERSION: &str = env!("CARGO_PKG_VERSION");
    const LAST_REL: &str = env!("LAST_RELEASE"); // e.g. "25/05/07|dac92|0.1.8"
    const GIT_SHA_CUR: &str = env!("GIT_SHA_SHORT"); // e.g. "d4e5f" or ""
    const BUILD_DATE: &str = env!("BUILD_DATE"); // e.g. "05/25"

    // Split LAST_RELEASE into (date, sha, version)
    let parts: Vec<_> = LAST_REL.split('|').collect();
    let (last_date, last_sha, last_version) = (
        parts.get(0).unwrap_or(&"00/00/00"),
        parts.get(1).unwrap_or(&"00000"),
        parts.get(2).unwrap_or(&"0.0.0"),
    );

    // Calculate days since last release
    let days_since_release =
        if let Ok(last_date_parsed) = NaiveDate::parse_from_str(last_date, "%d/%m/%y") {
            let today = Utc::now().naive_utc().date();
            (today - last_date_parsed).num_days()
        } else {
            -1 // Fallback if parsing fails
        };

    // Determine "custom" flag
    let custom_flag = if !GIT_SHA_CUR.is_empty() && GIT_SHA_CUR != *last_sha {
        format!(" |{}", GIT_SHA_CUR)
    } else {
        String::new()
    };

    // Build the window title
    let title = format!(
        "debugchrome:\\ {} | {} {}  {} {}d old",
        VERSION, last_sha, custom_flag, BUILD_DATE, days_since_release
    );

    let ret = eframe::run_native(
        &title,
        native_options,
        Box::new(|cc| Ok(Box::new(DemoApp::new(cc)))),
    );
    ret
}

async fn close_target(target_id: &str, url: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Connect to the Chrome DevTools Protocol WebSocket
    let response = reqwest::get("http://localhost:9222/json/version").await?;
    let version: serde_json::Value = response.json().await?;
    let ws_url = version["webSocketDebuggerUrl"]
        .as_str()
        .ok_or("No WebSocket URL found")?;

    let (mut ws_stream, _) = connect_async(ws_url).await?;
    println!("Connected to Chrome WebSocket: {}", ws_url);

    // Check if the URL is `chrome://profile-picker/`
    if url == "chrome://profile-picker/" {
        println!("Detected chrome://profile-picker/. Shutting down Chrome...");

        // Send the Browser.close command to shut down Chrome
        let close_browser_message = json!({
            "id": 1,
            "method": "Browser.close"
        });
        ws_stream
            .send(Message::Text(close_browser_message.to_string().into()))
            .await?;

        println!("Sent Browser.close command to Chrome.");
        return Ok(()); // Exit early since Chrome is shutting down
    }

    // Send the Target.closeTarget command
    let close_message = json!({
        "id": 1,
        "method": "Target.closeTarget",
        "params": {
            "targetId": target_id
        }
    });
    ws_stream
        .send(Message::Text(close_message.to_string().into()))
        .await?;

    // Wait for the response
    while let Some(msg) = ws_stream.next().await {
        if let Ok(Message::Text(response)) = msg {
            println!("Response: {}", response);
            break;
        }
    }

    Ok(())
}

use std::fs::{File, OpenOptions};

use fs2::FileExt;
use std::io::{Read, Write};
use std::process;
#[cfg(target_os = "windows")]
use winapi::shared::windef::HWND;
#[cfg(target_os = "windows")]
use winapi::um::winuser::SetForegroundWindow;
#[cfg(target_os = "windows")]
use winapi::um::winuser::{MB_ICONEXCLAMATION, MessageBeep};

static INSTANCE_LOCK: OnceCell<Option<File>> = OnceCell::new();
pub fn ensure_single_instance(hwnd: isize) -> Option<&'static File> {
    INSTANCE_LOCK.get_or_init(|| {
    let lock_file_path = "debugchrome.lock";

    // Attempt to open or create the lock file
    let mut lock_file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(lock_file_path)
    {
        Ok(file) => file,
        Err(e) => {
            log::error!("Failed to create or open lock file: {}", e);
            return None; // Exit gracefully if the lock file cannot be accessed
        }
    };

    // Try to lock the file exclusively
    if lock_file.try_lock_exclusive().is_err() {
        // If the lock is already held, check if the process is still running
        let mut contents = String::new();
        if let Err(e) = lock_file.read_to_string(&mut contents) {
            log::error!("Failed to read lock file: {}", e);
            return None; // Exit gracefully if the lock file cannot be read
        }

        #[cfg(target_os = "windows")]
        if let Some(pid) = contents.split_whitespace().next().and_then(|s| s.parse::<u32>().ok()) {
            if is_process_running(pid) {
                println!("Another instance is already running. Activating the existing instance...");
                unsafe {
                    // Play a Windows notification sound
                    MessageBeep(MB_ICONEXCLAMATION);
                }
                #[cfg(target_os = "windows")] {
                let hwnd = parse_lock_file_contents(&contents);
                activate_existing_window(hwnd);
                }
                process::exit(0);
            } else {
                println!("Stale lock file detected. Removing it...");
                if let Err(e) = std::fs::remove_file(lock_file_path) {
                    log::error!("Failed to remove stale lock file: {}", e);
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        if let Some(pid) = contents.split_whitespace().next().and_then(|s| s.parse::<u32>().ok()) {
            if is_process_running(pid) {
                println!("Another instance is already running. Activating the existing instance...");
                activate_existing_window(pid, hwnd as usize);
                process::exit(0);
            } else {
                println!("Stale lock file detected. Removing it...");
                if let Err(e) = std::fs::remove_file(lock_file_path) {
                    log::error!("Failed to remove stale lock file: {}", e);
                }
            }
        }
    }

    // Write the current process ID and HWND to the lock file
    let pid = process::id();
    //let hwnd = get_current_window_handle();
    if let Err(e) = lock_file.set_len(0) {
        log::error!("Failed to clear lock file: {}", e);
        return None; // Exit gracefully if the lock file cannot be cleared
    }
    if let Err(e) = writeln!(lock_file, "{} {}", pid, hwnd) {
        log::error!("Failed to write to lock file: {}", e);
        return None; // Exit gracefully if the lock file cannot be written to
    }
    Some(lock_file)
})
.as_ref()
}
#[cfg(target_os = "windows")]
use winapi::um::handleapi::CloseHandle;
#[cfg(target_os = "windows")]
use winapi::um::processthreadsapi::OpenProcess;
#[cfg(target_os = "windows")]
use winapi::um::winnt::PROCESS_QUERY_INFORMATION;

fn is_process_running(pid: u32) -> bool {
    #[cfg(target_os = "windows")]
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_INFORMATION, 0, pid);
        if handle.is_null() {
            false
        } else {
            CloseHandle(handle);
            true
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Implement process check for non-Windows platforms if needed
        false
    }
}
#[cfg(target_os = "windows")]
fn parse_lock_file_contents(contents: &str) -> HWND {
    contents
        .split_whitespace()
        .nth(1) // Get the second value (HWND)
        .and_then(|hwnd| hwnd.parse::<isize>().ok())
        .map(|hwnd| hwnd as HWND)
        .unwrap_or(std::ptr::null_mut())
}
fn get_current_window_handle() -> isize {
    use active_win_pos_rs::get_active_window;
    let hwnd = match get_active_window() {
        Ok(active_window) => {
            println!("Active window: {:?}", active_window);
            active_window
                .window_id
                .trim_start_matches("HWND(")
                .trim_end_matches(')')
                .parse::<isize>()
                .unwrap_or(0)
        }
        Err(()) => {
            println!("Error occurred while getting the active window");
            0 as isize
        }
    };
    println!("Current window handle: {:?}", hwnd);
    hwnd
    // unsafe {
    //     let hwnd = FindWindowA(std::ptr::null(), "DebugChrome GUI\0".as_ptr() as *const i8);
    //     hwnd as isize
    // }
}

#[cfg(target_os = "windows")]
fn activate_existing_window(hwnd: HWND) {
    unsafe {
        if hwnd.is_null() {
            println!("No valid window handle found. Unable to activate the existing instance.");
            return;
        }

        println!("Activating existing instance with HWND: {:?}", hwnd);
        SetForegroundWindow(hwnd);
    }
}

#[cfg(target_os = "linux")]
fn activate_existing_window(_pid: u32, _hwnd: usize) {
    use std::process::Command;

    // Use `wmctrl` to bring the existing window to the front
    Command::new("wmctrl")
        .args(&["-a", "DebugChrome GUI"])
        .output()
        .expect("Failed to activate existing window");
}

#[cfg(target_os = "macos")]
fn activate_existing_window(_pid: u32, _hwnd: usize) {
    use std::process::Command;

    // Use `osascript` to bring the existing window to the front
    Command::new("osascript")
        .arg("-e")
        .arg("tell application \"System Events\" to set frontmost of the first process whose name is \"DebugChrome GUI\" to true")
        .output()
        .expect("Failed to activate existing window");
}

#[derive(Debug)]
pub struct MonitoringState {
    pub is_running: AtomicBool, // Indicates if the monitoring process is running
    pub is_connected: AtomicBool, // Indicates if the monitoring process is connected
    pub notify: Arc<Notify>,    // Notify instance for signaling
}

impl MonitoringState {
    pub fn new() -> Self {
        Self {
            is_running: AtomicBool::new(false),
            is_connected: AtomicBool::new(false),
            notify: Arc::new(Notify::new()),
        }
    }

    pub fn set_running(&self, running: bool) {
        self.is_running.store(running, Ordering::SeqCst);
        if !running {
            self.notify.notify_waiters(); // Notify waiters when monitoring stops
        }
    }

    pub fn set_connected(&self, connected: bool) {
        self.is_connected.store(connected, Ordering::SeqCst);
    }

    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    pub fn is_connected(&self) -> bool {
        self.is_connected.load(Ordering::SeqCst)
    }

    pub fn get_notify(&self) -> Arc<Notify> {
        self.notify.clone()
    }
}

fn wait_for_browser_hwnd(pid: u32) -> io::Result<isize> {
    // Wait for the browser window to appear
    for _ in 0..10 {
        if let Some(hwnd) = find_hwnd_by_pid(pid) {
            return Ok(hwnd);
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Failed to find browser HWND",
    ))
}
fn find_hwnd_by_pid(pid: u32) -> Option<isize> {
    #[cfg(target_os = "windows")]
    {
        use winapi::um::winuser::{EnumWindows, GetWindowThreadProcessId};

        // Structure to hold both the target PID and the HWND
        struct EnumData {
            target_pid: u32,
            hwnd: isize,
        }

        unsafe extern "system" fn enum_windows_proc(
            hwnd: winapi::shared::windef::HWND,
            lparam: winapi::shared::minwindef::LPARAM,
        ) -> i32 {
            unsafe {
                let data = &mut *(lparam as *mut EnumData); // Cast lparam to EnumData
                let mut process_id = 0;
                GetWindowThreadProcessId(hwnd, &mut process_id);
                println!("Enumerating window: HWND={:?}, PID={}", hwnd, process_id);
                if process_id == data.target_pid {
                    println!("MATCH: HWND={:?}, PID={}", hwnd, process_id);
                    data.hwnd = hwnd as isize; // Update the HWND in EnumData
                    return 0; // Stop enumeration
                }
                1 // Continue enumeration
            }
        }

        let mut data = EnumData {
            target_pid: pid,
            hwnd: 0,
        };

        unsafe {
            EnumWindows(
                Some(enum_windows_proc),
                &mut data as *mut _ as winapi::shared::minwindef::LPARAM,
            );
        }

        println!("Final HWND: {:?}", data.hwnd);
        if data.hwnd != 0 {
            Some(data.hwnd)
        } else {
            None
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        None // Not implemented for non-Windows platforms
    }
}
