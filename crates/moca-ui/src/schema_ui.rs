use std::collections::{BTreeMap, HashMap};

use moca_protocol::{
    GameInterface, OptionDecl, OptionInputType, OptionKind, TaskDecl, TaskKind, TaskTier,
};

use crate::theme::{Palette, pill_badge, sub_card_frame, toggle_switch};

pub enum SchemaAction {
    RunBatchTask(TaskDecl),
    ToggleTrigger {
        task_id: String,
        task: TaskDecl,
        active: bool,
    },
}

/// Render realtime trigger switches and batch tasks declared by a `GameInterface`.
pub fn render_game_schema(
    ui: &mut egui::Ui,
    manifest: &GameInterface,
    selected_tasks: &mut Vec<String>,
    option_values: &mut HashMap<String, serde_json::Value>,
    active_triggers: &HashMap<String, (bool, u32)>,
) -> Option<SchemaAction> {
    let mut triggered = None;

    if let Some(welcome) = &manifest.welcome {
        ui.label(
            egui::RichText::new(welcome)
                .size(12.0)
                .color(Palette::TEXT_MUTED),
        );
        ui.add_space(4.0);
    }

    let mut grouped: BTreeMap<String, Vec<(&String, &TaskDecl)>> = BTreeMap::new();
    for (task_id, task) in &manifest.task {
        // Temporarily hide Cloud/VIP tasks in open-source desktop UI.
        if task.tier == TaskTier::Vip {
            continue;
        }
        let group_name = task.group.clone().unwrap_or_else(|| "默认任务".to_string());
        grouped.entry(group_name).or_default().push((task_id, task));
    }
    for tasks in grouped.values_mut() {
        tasks.sort_by(|a, b| a.0.cmp(b.0));
    }

    for (group_name, tasks) in grouped {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(group_name)
                .size(12.0)
                .strong()
                .color(Palette::TEXT_SECONDARY),
        );

        for (task_id, task) in tasks {
            if task.kind == TaskKind::Trigger {
                let (mut is_on, hit_count) =
                    active_triggers.get(task_id).copied().unwrap_or((false, 0));
                sub_card_frame().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        if toggle_switch(ui, &mut is_on).changed() {
                            triggered = Some(SchemaAction::ToggleTrigger {
                                task_id: task_id.clone(),
                                task: task.clone(),
                                active: is_on,
                            });
                        }

                        ui.label(egui::RichText::new(&task.label).size(13.5).strong().color(
                            if is_on {
                                Palette::TEXT_PRIMARY
                            } else {
                                Palette::TEXT_SECONDARY
                            },
                        ));

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if is_on {
                                let status_text = if hit_count > 0 {
                                    format!("● 运行中 ({hit_count})")
                                } else {
                                    "● 监听中".to_string()
                                };
                                pill_badge(
                                    ui,
                                    &status_text,
                                    Palette::SUCCESS,
                                    Palette::SUCCESS_BG,
                                    Palette::SUCCESS.gamma_multiply(0.35),
                                );
                            } else {
                                pill_badge(
                                    ui,
                                    "未开启",
                                    Palette::TEXT_MUTED,
                                    Palette::BG_CARD,
                                    Palette::BORDER,
                                );
                            }
                        });
                    });

                    if let Some(desc) = &task.description {
                        ui.label(
                            egui::RichText::new(desc)
                                .size(11.5)
                                .color(Palette::TEXT_MUTED),
                        );
                    }

                    if !task.option.is_empty() {
                        ui.add_space(2.0);
                        ui.separator();
                        ui.add_space(2.0);
                        for opt_key in &task.option {
                            if let Some(opt_decl) = manifest.option.get(opt_key) {
                                render_option_widget(ui, opt_key, opt_decl, option_values);
                            }
                        }
                    }
                });
                continue;
            }

            let mut enabled = selected_tasks.contains(task_id);
            sub_card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    if toggle_switch(ui, &mut enabled).changed() {
                        if enabled {
                            if !selected_tasks.contains(task_id) {
                                selected_tasks.push(task_id.clone());
                            }
                        } else {
                            selected_tasks.retain(|id| id != task_id);
                        }
                    }

                    ui.label(
                        egui::RichText::new(&task.label)
                            .size(13.5)
                            .color(if enabled {
                                Palette::TEXT_PRIMARY
                            } else {
                                Palette::TEXT_SECONDARY
                            }),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn = egui::Button::new(
                            egui::RichText::new("执行")
                                .size(12.0)
                                .color(Palette::TEXT_PRIMARY),
                        )
                        .min_size(egui::vec2(46.0, 24.0));
                        if ui.add(btn).clicked() {
                            triggered = Some(SchemaAction::RunBatchTask(task.clone()));
                        }
                    });
                });

                if let Some(desc) = &task.description {
                    ui.label(
                        egui::RichText::new(desc)
                            .size(11.5)
                            .color(Palette::TEXT_MUTED),
                    );
                }

                if enabled && !task.option.is_empty() {
                    ui.add_space(2.0);
                    ui.separator();
                    ui.add_space(2.0);
                    for opt_key in &task.option {
                        if let Some(opt_decl) = manifest.option.get(opt_key) {
                            render_option_widget(ui, opt_key, opt_decl, option_values);
                        }
                    }
                }
            });
        }
    }

    triggered
}

fn render_option_widget(
    ui: &mut egui::Ui,
    opt_key: &str,
    decl: &OptionDecl,
    values: &mut HashMap<String, serde_json::Value>,
) {
    let label = decl.label.as_deref().unwrap_or(opt_key);
    let current = values
        .entry(opt_key.to_string())
        .or_insert_with(|| decl.default.clone().unwrap_or(serde_json::Value::Null));

    match decl.kind {
        OptionKind::Boolean => {
            let mut val = current.as_bool().unwrap_or(false);
            ui.horizontal(|ui| {
                if toggle_switch(ui, &mut val).changed() {
                    *current = serde_json::Value::Bool(val);
                }
                ui.label(egui::RichText::new(label).color(Palette::TEXT_SECONDARY));
            });
        }
        OptionKind::Select => {
            let selected_text = decl
                .items
                .iter()
                .find(|item| {
                    let item_val = item
                        .value
                        .as_ref()
                        .cloned()
                        .unwrap_or_else(|| serde_json::Value::String(item.name.clone()));
                    &item_val == current
                })
                .map(|item| item.label.as_deref().unwrap_or(&item.name).to_string())
                .unwrap_or_else(|| "请选择".to_string());

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(label).color(Palette::TEXT_SECONDARY));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::ComboBox::from_id_salt(format!("combo_{opt_key}"))
                        .width(ui.available_width().min(220.0))
                        .selected_text(selected_text)
                        .show_ui(ui, |ui| {
                            for item in &decl.items {
                                let item_val = item.value.as_ref().cloned().unwrap_or_else(|| {
                                    serde_json::Value::String(item.name.clone())
                                });
                                let text = item.label.as_deref().unwrap_or(&item.name);
                                if ui.selectable_label(&item_val == current, text).clicked() {
                                    *current = item_val;
                                }
                            }
                        });
                });
            });
        }
        OptionKind::Checkbox => {
            ui.label(egui::RichText::new(label).color(Palette::TEXT_SECONDARY));
            let mut arr = current.as_array().cloned().unwrap_or_default();
            ui.horizontal_wrapped(|ui| {
                for item in &decl.items {
                    let item_val = item
                        .value
                        .as_ref()
                        .cloned()
                        .unwrap_or_else(|| serde_json::Value::String(item.name.clone()));
                    let mut is_on = arr.contains(&item_val);
                    let text = item.label.as_deref().unwrap_or(&item.name);
                    if ui.checkbox(&mut is_on, text).changed() {
                        if is_on && !arr.contains(&item_val) {
                            arr.push(item_val);
                        } else if !is_on {
                            arr.retain(|v| v != &item_val);
                        }
                        *current = serde_json::Value::Array(arr.clone());
                    }
                }
            });
        }
        OptionKind::Input => {
            let mut text = current.as_str().unwrap_or_default().to_string();
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(label).color(Palette::TEXT_SECONDARY));
                let mut edit =
                    egui::TextEdit::singleline(&mut text).desired_width(ui.available_width());
                if decl.input_type == OptionInputType::Password {
                    edit = edit.password(true);
                }
                if ui.add(edit).changed() {
                    *current = serde_json::Value::String(text);
                }
            });
        }
    }
}
