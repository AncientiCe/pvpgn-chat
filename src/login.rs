use eframe::egui::{self, Color32, TextEdit};
use serde::{Deserialize, Serialize};
use crate::Credentials;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Login {
    pub server: String,
    pub user: String,
    #[serde(skip)]
    pub password: String,
    pub error: Option<String>,
}

impl Default for Login {
    fn default() -> Self {
        let mut login = Login {
            server: "".to_string(),
            user: "".to_string(),
            password: "".to_string(),
            error: None,
        };
        if let Ok(text) = std::fs::read_to_string(&"credentials.json") {

                // Parse the string into a dynamically-typed JSON structure.
            let credentials = serde_json::from_str::<Credentials>(&text).unwrap();
            login.user.push_str(&credentials.user);
            login.server.push_str(&credentials.server);
            login.password.push_str(&credentials.password);
        };

        login
    }
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
                    ui.vertical_centered(|ui| {
                        ui.heading("Log in");
                        ui.label("Server ip and port:");
                        ui.text_edit_singleline(&mut self.server);

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