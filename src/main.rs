use std::{fs, thread};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, mpsc, Mutex};
use std::thread::JoinHandle;
use egui::{Color32, Context, RichText};
use egui_file::FileDialog;
use geo_aid_internal::engine::rage::Rage;
use geo_aid_internal::projector;
use geo_aid_internal::projector::figure::{Item, Label, Position};
use geo_aid_internal::script::figure::{Figure, Generated};
use geo_aid_internal::script::math;
use geo_aid_internal::script::math::{Flags, Intermediate};
use macroquad::prelude::*;

mod egui_macroquad;
mod egui_miniquad;

struct Compiled {
    intermediate: Intermediate,
    max_adjustment: f64,
    rage: Rage,
    flags: Arc<Flags>
}

enum Message {
    Next,
    Quit
}

struct Runtime {
    control: mpsc::Sender<Message>,
    flags: Arc<Flags>,
    generated: Arc<Mutex<Generated>>,
    handle: JoinHandle<()>
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.control.send(Message::Quit).unwrap();
    }
}

fn runtime(
    mut rage: Rage,
    control: mpsc::Receiver<Message>,
    max_adjustment: f64,
    figure: &Figure,
    generated: Arc<Mutex<Generated>>,
) {
    let magnitudes = rage.gen().bake_magnitudes(max_adjustment);

    loop {
        match control.recv().unwrap() {
            Message::Quit => break,
            Message::Next => {
                rage.gen_mut().cycle_prebaked(&magnitudes);
                let fig = rage.get_figure(figure.clone());
                *generated.lock().unwrap() = fig;
            }
        }
    }
}

struct Debugger {
    dialog: FileDialog,
    file: Option<PathBuf>,
    file_valid: bool,
    worker_count: String,
    worker_count_valid: bool,
    max_adjustment: String,
    max_adjustment_valid: bool,
    runtime: Option<Runtime>,
    run: bool
}

impl Debugger {
    #[must_use]
    pub fn new() -> Self {
        let mut dialog = FileDialog::open_file(None);

        Self {
            dialog,
            file: None,
            file_valid: true,
            worker_count: String::from("512"),
            worker_count_valid: true,
            max_adjustment: String::from("0.5"),
            max_adjustment_valid: true,
            runtime: None,
            run: false
        }
    }

    pub fn show(&mut self, ctx: &Context) {
        egui::Window::new("Start generating")
            .show(ctx, |ui| {
                let mut quit = false;

                if let Some(runtime) = &self.runtime {
                    if ui.button("Quit").clicked() {
                        quit = true;
                    }

                    if self.run {
                        if ui.button("Stop").clicked() {
                            self.run = false;
                        } else {
                            runtime.control.send(Message::Next).unwrap();
                        }
                    } else {
                        if ui.button("Run").clicked() {
                            self.run = true;
                        }

                        if ui.button("Next step").clicked() {
                            runtime.control.send(Message::Next).unwrap();
                        }
                    }
                } else {
                    egui::Grid::new("file-data")
                        .num_columns(2)
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label("File:");
                            if let Some(file) = &self.file {
                                ui.horizontal(|ui| {
                                    ui.label(file.to_string_lossy());

                                    if ui.button("Change").clicked() {
                                        self.dialog.open();
                                    }
                                });
                            } else {
                                if ui.button("Open").clicked() {
                                    self.dialog.open();
                                }
                            }
                            ui.end_row();

                            if !self.file_valid {
                                ui.label(RichText::new("Invalid file").color(Color32::RED));
                                ui.end_row();
                            }

                            ui.label("Worker count:");
                            ui.text_edit_singleline(&mut self.worker_count);
                            ui.end_row();

                            if !self.worker_count_valid {
                                ui.label(RichText::new("Invalid worker count").color(Color32::RED));
                                ui.label("Must be positive integer.");
                                ui.end_row();
                            }

                            ui.label("Maximum adjustment:");
                            ui.text_edit_singleline(&mut self.max_adjustment);
                            ui.end_row();

                            if !self.max_adjustment_valid {
                                ui.label(RichText::new("Invalid max adjustment").color(Color32::RED));
                                ui.label("Must be a positive float");
                                ui.end_row();
                            }

                            ui.label("");
                            if ui.button("Generate").clicked() {
                                let wc = usize::from_str(&self.worker_count).ok();
                                let ma = f64::from_str(&self.max_adjustment).ok();
                                let file = self.file.as_ref()
                                    .and_then(|file| fs::read_to_string(file).ok())
                                    .and_then(|file| math::load_script(&file).ok());

                                self.file_valid = file.is_some();
                                self.worker_count_valid = wc.is_some();
                                self.max_adjustment_valid = ma.is_some();

                                if let Some(wc) = wc {
                                    if let Some(ma) = ma {
                                        if let Some(file) = file {
                                            let rage = Rage::new(wc, &file);
                                            let flags = Arc::new(file.flags.clone());
                                            let generated = Arc::new(Mutex::new(Generated::default()));
                                            let gen2 = Arc::clone(&generated);

                                            let (send, recv) = mpsc::channel();

                                            self.runtime = Some(Runtime {
                                                control: send,
                                                flags,
                                                generated,
                                                handle: thread::spawn(move || {
                                                    runtime(rage, recv, ma, &file.figure, gen2)
                                                })
                                            });
                                        }
                                    }
                                }
                            }
                            ui.end_row();
                        });
                }

                if quit {
                    self.run = false;
                    self.runtime = None;
                }
            });

        if self.dialog.show(ctx).selected() {
            if let Some(path) = self.dialog.path() {
                self.file = Some(path.to_path_buf());
            }
        }
    }
}

fn window_conf() -> Conf {
    Conf {
        window_resizable: true,
        window_title: String::from("Geo-AID Debugger"),
        ..Conf::default()
    }
}

fn draw_label(label: &Option<Label>) {
    if let Some(label) = label {
        draw_text(
            &label.content.to_string(),
            label.position.x as f32,
            label.position.y as f32,
            18.0,
            BLACK
        );
    }
}

fn draw_points(points: &(Position, Position)) {
    draw_line(
        points.0.x as f32,
        points.0.y as f32,
        points.1.x as f32,
        points.1.y as f32,
        1.0,
        BLACK
    );
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut debugger = Debugger::new();

    loop {
        clear_background(WHITE);

        if let Some(dbg) = &debugger.runtime {
            let fig = dbg.generated.lock().unwrap();
            let figure = projector::project(
                fig.clone(),
                &dbg.flags,
                (
                    screen_width() as usize - 300,
                    screen_height() as usize
                )
            );

            for item in &figure.items {
                match item {
                    Item::Point(pt) => {
                        if pt.display_dot {
                            draw_circle(pt.position.x as f32, pt.position.y as f32, 2.0, BLACK);
                        }
                        draw_label(&pt.label);
                    }
                    Item::Line(ln) => {
                        draw_points(&ln.points);
                        draw_label(&ln.label);
                    }
                    Item::Segment(x)
                    | Item::Ray(x) => {
                        draw_points(&x.points);
                        draw_label(&x.label);
                    }
                    Item::Circle(circle) => {
                        draw_circle_lines(
                            circle.center.x as f32,
                            circle.center.y as f32,
                            circle.radius as f32,
                            1.0, BLACK
                        );
                        draw_label(&circle.label);
                    }
                }
            }
        }

        egui_macroquad::ui(|ctx| {
            debugger.show(ctx);
        });

        egui_macroquad::draw();

        next_frame().await;
    }
}
