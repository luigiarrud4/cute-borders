use eframe::egui;
use crate::config::{Config, Rule, RuleMatch};
use crate::logger::Logger;

struct ConfigApp {
    is_rainbow_active: bool,
    is_inactive_disabled: bool, // <- Novo estado para o checkbox
    active_color_hex: String,
    inactive_color_hex: String,
    active_color_picker: egui::Color32,
    inactive_color_picker: egui::Color32,
    rainbow_speed: f32,
}

impl ConfigApp {
    fn hex_to_color32(hex: &str) -> egui::Color32 {
        egui::Color32::from_hex(hex).unwrap_or(egui::Color32::BLACK)
    }

    fn color32_to_hex(color: egui::Color32) -> String {
        format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b())
    }

    fn load_from_config() -> Self {
        let config = Config::read_for_gui();
        let mut active_hex = "#FFFFFF".to_string();
        let mut inactive_hex = "#444444".to_string();
        let mut is_rainbow = false;
        let mut is_inactive_disabled = false;

        if let Some(global_rule) = config.window_rules.iter().find(|r| r.rule_match == RuleMatch::Global) {
            // Lógica da Cor Inativa
            is_inactive_disabled = global_rule.inactive_border_color.is_empty();
            if !is_inactive_disabled {
                inactive_hex = global_rule.inactive_border_color.clone();
            }

            // Lógica da Cor Ativa
            is_rainbow = global_rule.active_border_color.to_lowercase() == "rainbow";
            if !is_rainbow {
                active_hex = global_rule.active_border_color.clone();
            }
        }

        Self {
            is_rainbow_active: is_rainbow,
            is_inactive_disabled,
            active_color_hex: active_hex.clone(),
            inactive_color_hex: inactive_hex.clone(),
            active_color_picker: Self::hex_to_color32(&active_hex),
            inactive_color_picker: Self::hex_to_color32(&inactive_hex),
            rainbow_speed: config.rainbow_speed.unwrap_or(1.0),
        }
    }

    fn save_to_config(&self) {
        let mut config = Config::read_for_gui();
        config.rainbow_speed = Some(self.rainbow_speed);

        let active_color = if self.is_rainbow_active { "rainbow".to_string() } else { self.active_color_hex.clone() };
        
        // Se o checkbox estiver marcado, salve uma string vazia. Senão, salve a cor.
        let inactive_color = if self.is_inactive_disabled { "".to_string() } else { self.inactive_color_hex.clone() };

        if let Some(global_rule) = config.window_rules.iter_mut().find(|r| r.rule_match == RuleMatch::Global) {
            global_rule.active_border_color = active_color;
            global_rule.inactive_border_color = inactive_color;
        } else {
            config.window_rules.push(Rule {
                rule_match: RuleMatch::Global, contains: None,
                active_border_color: active_color,
                inactive_border_color: inactive_color,
            });
        }
        if let Err(e) = Config::write_config(&config) {
            Logger::log(&format!("[GUI ERROR] Failed to write config: {:?}", e));
        } else {
            Logger::log("[GUI] Config saved successfully.");
        }
    }
}

impl eframe::App for ConfigApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Cute Borders - Configurações");
            ui.separator();
            ui.add_space(5.0);

            egui::Grid::new("config_grid")
                .num_columns(2)
                .spacing([40.0, 8.0])
                .show(ui, |ui| {
                    
                    ui.label("Cor da Borda Ativa:");
                    ui.add_enabled_ui(!self.is_rainbow_active, |ui| {
                        ui.horizontal(|ui| {
                            let color_picker_response = ui.color_edit_button_srgba(&mut self.active_color_picker);
                            let text_edit_response = ui.text_edit_singleline(&mut self.active_color_hex);
                            if color_picker_response.changed() { self.active_color_hex = Self::color32_to_hex(self.active_color_picker); }
                            if text_edit_response.changed() { self.active_color_picker = Self::hex_to_color32(&self.active_color_hex); }
                        });
                    });
                    ui.end_row();

                    ui.label("");
                    ui.checkbox(&mut self.is_rainbow_active, "Modo Rainbow (RGB)");
                    ui.end_row();

                    if self.is_rainbow_active {
                        ui.label("Velocidade do Rainbow:");
                        ui.add(egui::Slider::new(&mut self.rainbow_speed, 0.1..=10.0));
                        ui.end_row();
                    }
                    
                    ui.label("Cor da Borda Inativa:");
                    // Os controles de cor inativa agora ficam desabilitados se o checkbox estiver marcado
                    ui.add_enabled_ui(!self.is_inactive_disabled, |ui| {
                        ui.horizontal(|ui| {
                            let color_picker_response = ui.color_edit_button_srgba(&mut self.inactive_color_picker);
                            let text_edit_response = ui.text_edit_singleline(&mut self.inactive_color_hex);
                            if color_picker_response.changed() { self.inactive_color_hex = Self::color32_to_hex(self.inactive_color_picker); }
                            if text_edit_response.changed() { self.inactive_color_picker = Self::hex_to_color32(&self.inactive_color_hex); }
                        });
                    });
                    ui.end_row();

                    // --- CHECKBOX ADICIONADO AQUI ---
                    ui.label(""); // Célula vazia para alinhamento
                    ui.checkbox(&mut self.is_inactive_disabled, "Desativar borda inativa");
                    ui.end_row();
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            ui.horizontal(|ui|{
                if ui.button("Salvar").clicked() { self.save_to_config(); }
                if ui.button("Salvar e Fechar").clicked() {
                    self.save_to_config();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });

            ui.add_space(10.0);
            ui.label("Nota: As mudanças são aplicadas automaticamente ao salvar!");
        });
    }
}

pub fn run_gui() {
    let viewport = egui::ViewportBuilder::default().with_inner_size([675.0, 617.0]);
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("Configurações", options, Box::new(|_cc| Box::new(ConfigApp::load_from_config()))).ok();
}