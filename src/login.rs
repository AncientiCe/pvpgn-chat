use eframe::egui::{self, Color32, TextEdit};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Login {
    pub server: String,
    pub user: String,
    #[serde(skip)]
    pub password: String,
    pub error: Option<String>,
}

impl Login {
    pub fn update(&mut self, ctx: &egui::Context) -> bool {
        let mut update = false;
        egui::CentralPanel::default().show(ctx, |ui| {

            ui.vertical_centered(|ui| {
                //ui.style_mut() .visuals .widgets .noninteractive .bg_stroke .color = egui::Color32::TRANSPARENT;
                ui.heading("Login");
                ui.add_space(10.0);
                ui.group(|ui| {
                    //ui.reset_style();

                    if let Some(ref error) = self.error {
                        ui.colored_label(Color32::from_rgb(255, 0, 0), error);
                    }
                    ui.vertical(|ui| {
                        ui.heading("Log in");
                        ui.label("Server ip and port:");
                        ui.add(
                            TextEdit::singleline(&mut self.server)
                                .hint_text("116.203.95.137:6112 - For eurobattle.net"),
                        );
                        ui.label("Username:");
                        ui.add(TextEdit::singleline(&mut self.user).hint_text("alice"));
                        ui.label("Password:");
                        ui.add(TextEdit::singleline(&mut self.password).password(true));
                        if ui.button("Log in").clicked() {
                            update = true;
                        }
                    })
                })
            });
        });
        update
    }
}