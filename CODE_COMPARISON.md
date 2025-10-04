# Code Comparison: egui vs Slint

## UI Definition

### egui (Imperative - All in Rust)

```rust
impl eframe::App for FerrisUnzipApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("FerrisUnzip - Archive Extractor");
            
            ui.add_space(20.0);
            
            ui.horizontal(|ui| {
                ui.label("Archive file:");
                if ui.button("Browse...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Archives", &["zip", "7z", "tar", "gz", "bz2", "xz", "rar"])
                        .pick_file()
                    {
                        self.archive_path = path.display().to_string();
                    }
                }
            });
            
            ui.horizontal(|ui| {
                ui.label("Path:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.archive_path)
                        .desired_width(350.0)
                );
            });
            
            // ... more UI code ...
        });
    }
}
```

### Slint (Declarative - Separate .slint file)

**ui/appwindow.slint:**
```slint
export component AppWindow inherits Window {
    title: "FerrisUnzip - Archive Extractor";
    
    in-out property <string> archive-path;
    
    callback browse-archive();
    
    VerticalBox {
        Text {
            text: "FerrisUnzip - Archive Extractor";
            font-size: 18px;
            font-weight: 700;
        }
        
        HorizontalBox {
            Text {
                text: "Archive file:";
                width: 100px;
            }
            
            Button {
                text: "Browse...";
                clicked => { browse-archive(); }
            }
        }
        
        HorizontalBox {
            Text {
                text: "Path:";
                width: 100px;
            }
            
            LineEdit {
                text <=> archive-path;
            }
        }
    }
}
```

**src/main.rs:**
```rust
fn run_gui(archive_file: Option<String>) -> Result<(), Box<dyn Error>> {
    let ui = AppWindow::new()?;
    
    ui.on_browse_archive(move || {
        if let Some(ui) = ui_weak.upgrade() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Archives", &["zip", "7z", "tar", "gz", "bz2", "xz", "rar"])
                .pick_file()
            {
                ui.set_archive_path(path.display().to_string().into());
            }
        }
    });
    
    ui.run()?;
    Ok(())
}
```

## Status Messages with Colors

### egui (Computed at render time)

```rust
let status_color = if self.status_message.starts_with("✓") {
    egui::Color32::from_rgb(0, 150, 0)
} else if self.status_message.starts_with("✗") {
    egui::Color32::from_rgb(200, 0, 0)
} else if self.status_message.contains("Extracting") {
    egui::Color32::from_rgb(0, 100, 200)
} else {
    egui::Color32::from_rgb(100, 100, 100)
};

ui.colored_label(status_color, &self.status_message);
```

### Slint (Boolean properties)

**ui/appwindow.slint:**
```slint
in-out property <bool> status-success: false;
in-out property <bool> status-error: false;
in-out property <bool> status-extracting: false;

Text {
    text: status-message;
    color: status-success ? #009600 : 
           (status-error ? #c80000 : 
           (status-extracting ? #0064c8 : #646464));
}
```

**src/main.rs:**
```rust
ui.set_status_success(true);
ui.set_status_error(false);
ui.set_status_extracting(false);
```

## Progress Updates

### egui (Direct state mutation)

```rust
struct FerrisUnzipApp {
    progress: Arc<Mutex<f32>>,
    progress_message: Arc<Mutex<String>>,
}

// In render loop:
if self.is_extracting {
    let progress = *self.progress.lock().unwrap();
    ui.add(egui::ProgressBar::new(progress / 100.0));
}

// In callback:
*self.progress.lock().unwrap() = progress;
*self.progress_message.lock().unwrap() = message;
ctx.request_repaint(); // Must explicitly request repaint
```

### Slint (Property binding with automatic updates)

**ui/appwindow.slint:**
```slint
if is-extracting: VerticalBox {
    ProgressIndicator {
        progress: progress / 100.0;
    }
    
    Text {
        text: progress-message;
    }
}
```

**src/main.rs:**
```rust
let progress = Arc::new(Mutex::new(0.0f32));

// In callback:
if let Some(ui_inner) = ui_weak_inner.upgrade() {
    ui_inner.set_progress(prog);
    ui_inner.set_progress_message(message.into());
    // Slint automatically repaints when properties change
}
```

## Key Differences Summary

| Aspect | egui | Slint |
|--------|------|-------|
| **UI Definition** | Imperative Rust code | Declarative .slint files |
| **Code Location** | Mixed with logic | Separate from logic |
| **Updates** | Manual repaint requests | Automatic on property change |
| **State Management** | Custom structs | Properties with bindings |
| **Styling** | Inline in code | Declarative in UI file |
| **Learning Curve** | Rust developers | Designers + developers |
| **Maintainability** | Harder to visualize | Easier to understand |
| **IDE Support** | Standard Rust | Slint LSP available |

## Lines of Code

- **egui version**: ~1,037 lines (all in main.rs)
- **Slint version**: 913 lines (main.rs) + 231 lines (appwindow.slint) = 1,144 total
- **Difference**: +107 lines (+10%), but with better separation

The small increase in total lines is offset by much better organization and maintainability.
