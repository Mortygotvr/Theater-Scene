#include <QtWidgets/QApplication>
#include <QtWidgets/QMainWindow>
#include <QtWidgets/QDockWidget>
#include <QtWidgets/QDialog>
#include <QtWidgets/QMessageBox>
#include <QtWidgets/QVBoxLayout>
#include <QtWidgets/QHBoxLayout>
#include <QtWidgets/QLabel>
#include <QtWidgets/QLineEdit>
#include <QtWidgets/QCheckBox>
#include <QtWidgets/QSlider>
#include <QtWidgets/QPushButton>
#include <QtWidgets/QScrollArea>
#include <QtWidgets/QComboBox>
#include <QtWidgets/QFrame>
#include <QtWidgets/QMenuBar>
#include <QtWidgets/QMenu>
#include <QtCore/QString>
#include <QtCore/QPointer>
#include <QtCore/QTimer>
#include <QtWidgets/QTableWidget>
#include <QtWidgets/QHeaderView>
#include <QtGui/QAction>
#include <QtGui/QPainter>
#include <QtGui/QImage>
#include <QtGui/QMouseEvent>

#include <dlfcn.h>
#include <cstdio>
#include <cstring>
#include <vector>
#include <unordered_map>

// ============================================================================
// 🔌 OBS Frontend Symbols (Weakly bound)
// ============================================================================
extern "C" {
    void* obs_frontend_get_main_window(void) __attribute__((weak));
    typedef void (*obs_frontend_add_custom_qdock_t)(const char *id, void *dock);
    uint32_t trust_studio_render_preview(uint32_t canvas_w, uint32_t canvas_h, uint8_t *out_rgba, size_t max_len) __attribute__((weak));
}

static void* get_obs_frontend_lib() {
    void *lib = dlopen("libobs-frontend-api.so.0", RTLD_NOLOAD | RTLD_LAZY);
    if (!lib) lib = dlopen("libobs-frontend-api.so", RTLD_NOLOAD | RTLD_LAZY);
    if (!lib) lib = dlopen("libobs-frontend-api.so.1", RTLD_NOLOAD | RTLD_LAZY);
    return lib;
}

static QMainWindow* find_obs_main_window() {
    if (obs_frontend_get_main_window) {
        if (void *w = obs_frontend_get_main_window()) {
            return reinterpret_cast<QMainWindow*>(w);
        }
    }
    if (qApp) {
        for (QWidget *w : QApplication::topLevelWidgets()) {
            if (QMainWindow *mw = qobject_cast<QMainWindow*>(w)) {
                return mw;
            }
        }
    }
    return nullptr;
}

static void ensure_qapp() {
    if (!qApp) {
        static int argc = 1;
        static char app_name[] = "obs-theater-plugin";
        static char *argv[] = { app_name, nullptr };
        new QApplication(argc, argv);
    }
}

// ============================================================================
// 🧱 Generic Lua/C callback typedefs used by the widget toolkit below
// ============================================================================
typedef void (*LuaBtnCallback)(void *user_data);
typedef void (*LuaSliderCallback)(double value, void *user_data);
typedef void (*LuaTextCallback)(const char *text, void *user_data);
typedef void (*LuaCheckCallback)(bool checked, void *user_data);
typedef void (*LuaComboboxCallback)(int selected_idx, const char *selected_text, void *user_data);
typedef void (*LuaTableCheckCallback)(int row, int col, bool checked, void *user_data);
typedef void (*LuaTableSelectCallback)(int row, const char *name, void *user_data);

// ============================================================================
// 🧱 Universal Dynamic UI Widgets & Registry for Lua
// ============================================================================
struct DynamicWidgetNode {
    uint32_t handle = 0;
    QPointer<QWidget> widget;
    QPointer<QLabel> valLabel;
    double scaleFactor = 1.0;
    QString suffix = "";
    LuaSliderCallback sliderCb = nullptr;
    LuaBtnCallback btnCb = nullptr;
    LuaTextCallback textCb = nullptr;
    LuaCheckCallback checkCb = nullptr;
    void *userData = nullptr;
};

static uint32_t g_next_ui_handle = 1;
static std::unordered_map<uint32_t, DynamicWidgetNode> g_dynamic_widgets;
static std::unordered_map<uint32_t, QPointer<QWidget>> g_universal_containers;
static std::unordered_map<uint32_t, QPointer<QDialog>> g_universal_windows;
static std::unordered_map<uint32_t, QPointer<QDockWidget>> g_universal_docks;

// ============================================================================
// 🎬 Custom top-level menu (QMenuBar) — for a dedicated menu instead of
// nesting under OBS's built-in "Tools" menu. Real QMenuBar manipulation, so
// unlike obs_frontend_add_tools_menu_item this must be called on the Qt UI
// thread (the caller, e.g. Rust via obs_queue_task(OBS_TASK_UI, ...), is
// responsible for that).
// ============================================================================
static QPointer<QMenu> g_top_menu;
static QString g_current_menu_title = "🎬 Theater Scene";

extern "C" {

void trust_ui_set_menu_title(const char *menu_title) {
    if (!menu_title || !*menu_title) return;
    g_current_menu_title = QString::fromUtf8(menu_title);

    QMainWindow *mainWin = find_obs_main_window();
    if (mainWin && mainWin->menuBar()) {
        QMenuBar *menuBar = mainWin->menuBar();
        if (g_top_menu) {
            g_top_menu->setTitle(g_current_menu_title);
        } else {
            g_top_menu = menuBar->addMenu(g_current_menu_title);
        }
        menuBar->update();
    }
}

void trust_ui_clear_menu() {
    if (g_top_menu) {
        g_top_menu->clear();
    }
}

void trust_ui_add_menu_action(const char *action_title, LuaBtnCallback callback, void *user_data) {
    if (!action_title) return;
    QMainWindow *mainWin = find_obs_main_window();
    if (mainWin && mainWin->menuBar()) {
        QMenuBar *menuBar = mainWin->menuBar();
        if (!g_top_menu) {
            g_top_menu = menuBar->addMenu(g_current_menu_title);
        }
        if (g_top_menu) {
            QAction *act = g_top_menu->addAction(QString::fromUtf8(action_title));
            if (callback) {
                QObject::connect(act, &QAction::triggered, [callback, user_data]() {
                    if (callback) callback(user_data);
                });
            }
            menuBar->update();
        }
    }
}

}

// ============================================================================
// 🌟 Universal UI Engine C-FFI Exports (Windows, Docks, Controls)
// Reusable general-purpose Qt widget toolkit, callable from Rust (or any
// other host) via dlopen/dlsym. Not an OBS module itself — see lib.rs's
// obs_module_load/obs_frontend_add_tools_menu_item for the actual OBS
// integration (menu item, plugin lifecycle).
// ============================================================================
extern "C" {

uint32_t trust_ui_create_window(const char *window_id, const char *title, int width, int height) {
    ensure_qapp();
    QMainWindow *mainWin = find_obs_main_window();
    QString wTitle = (title && *title) ? QString::fromUtf8(title) : "🎬 Window";
    QString wId = (window_id && *window_id) ? QString::fromUtf8(window_id) : "custom_window";

    QDialog *win = new QDialog(mainWin, Qt::Window);
    win->setWindowTitle(wTitle);
    win->setObjectName(wId);
    win->resize(width > 0 ? width : 640, height > 0 ? height : 480);
    win->setStyleSheet("QDialog { background-color: #0f172a; color: #f8fafc; font-family: 'Segoe UI', sans-serif; }");

    QVBoxLayout *layout = new QVBoxLayout(win);
    layout->setContentsMargins(12, 12, 12, 12);
    layout->setSpacing(8);

    uint32_t handle = g_next_ui_handle++;
    g_universal_windows[handle] = win;
    g_universal_containers[handle] = win;
    return handle;
}

typedef void (*obs_frontend_add_dock_by_id_t)(const char *id, const char *title, void *widget);

uint32_t trust_ui_create_dock(const char *dock_id, const char *title) {
    ensure_qapp();
    QMainWindow *mainWin = find_obs_main_window();
    QString dTitle = (title && *title) ? QString::fromUtf8(title) : "🎬 Dock";
    QString dId = (dock_id && *dock_id) ? QString::fromUtf8(dock_id) : "custom_dock";

    // obs_frontend_add_dock_by_id takes the plain content widget and creates
    // + owns the QDockWidget wrapper (placement, View-menu registration,
    // layout persistence) itself — no manual QDockWidget/addDockWidget needed.
    QWidget *dockContent = new QWidget(mainWin);
    dockContent->setObjectName(dId);
    // Deliberately no setStyleSheet here — leave this widget's palette alone
    // so it inherits OBS's own theme (light/dark, whatever the user has
    // picked) instead of a fixed custom color scheme.
    QVBoxLayout *layout = new QVBoxLayout(dockContent);
    layout->setContentsMargins(10, 10, 10, 10);
    layout->setSpacing(8);

    (void)mainWin;
    void *lib_handle = get_obs_frontend_lib();
    if (lib_handle) {
        obs_frontend_add_dock_by_id_t add_dock_fn = (obs_frontend_add_dock_by_id_t)dlsym(lib_handle, "obs_frontend_add_dock_by_id");
        if (add_dock_fn) {
            add_dock_fn(dId.toUtf8().constData(), dTitle.toUtf8().constData(), (void*)dockContent);
        }
    }

    uint32_t handle = g_next_ui_handle++;
    g_universal_containers[handle] = dockContent;
    return handle;
}

uint32_t trust_ui_add_tab(uint32_t container_handle, const char *title, const char *icon) {
    auto it = g_universal_containers.find(container_handle);
    if (it == g_universal_containers.end() || !it->second) return 0;

    QWidget *parentWidget = it->second.data();
    QLayout *parentLayout = parentWidget->layout();
    if (!parentLayout) return 0;

    QTabWidget *tabHost = parentWidget->findChild<QTabWidget*>();
    if (!tabHost) {
        tabHost = new QTabWidget(parentWidget);
        tabHost->setStyleSheet("QTabWidget::pane { border: 1px solid #334155; background: #0f172a; } QTabBar::tab { background: #1e293b; color: #94a3b8; padding: 6px 14px; margin-right: 2px; } QTabBar::tab:selected { background: #0f172a; color: #38bdf8; font-weight: bold; border-top: 2px solid #38bdf8; }");
        parentLayout->addWidget(tabHost);
    }

    QWidget *tabWidget = new QWidget();
    QVBoxLayout *tabLayout = new QVBoxLayout(tabWidget);
    tabLayout->setContentsMargins(6, 6, 6, 6);
    tabLayout->setSpacing(6);

    QScrollArea *scroll = new QScrollArea(tabWidget);
    scroll->setWidgetResizable(true);
    QWidget *scrollContent = new QWidget();
    QVBoxLayout *scrollLayout = new QVBoxLayout(scrollContent);
    scrollLayout->setContentsMargins(0, 0, 0, 0);
    scrollLayout->setSpacing(6);
    scroll->setWidget(scrollContent);
    tabLayout->addWidget(scroll);

    QString tTitle = QString::fromUtf8(title ? title : "Tab");
    QString tIcon = QString::fromUtf8(icon ? icon : "🎯");
    tabHost->addTab(tabWidget, QString("%1 %2").arg(tIcon).arg(tTitle));

    uint32_t handle = g_next_ui_handle++;
    g_universal_containers[handle] = scrollContent;
    return handle;
}

uint32_t trust_ui_add_card(uint32_t container_handle, const char *card_title) {
    auto it = g_universal_containers.find(container_handle);
    if (it == g_universal_containers.end() || !it->second) return 0;

    QWidget *parentWidget = it->second.data();
    QLayout *parentLayout = parentWidget->layout();
    if (!parentLayout) return 0;

    QFrame *card = new QFrame(parentWidget);
    card->setStyleSheet("QFrame { background: #1e293b; border: 1px solid #334155; border-radius: 6px; padding: 6px; }");
    QVBoxLayout *cardLayout = new QVBoxLayout(card);
    cardLayout->setContentsMargins(6, 6, 6, 6);
    cardLayout->setSpacing(6);

    if (card_title && *card_title) {
        QLabel *lbl = new QLabel(QString::fromUtf8(card_title), card);
        lbl->setStyleSheet("font-size: 10px; font-weight: bold; color: #38bdf8; letter-spacing: 0.5px;");
        cardLayout->addWidget(lbl);
    }

    parentLayout->addWidget(card);

    uint32_t handle = g_next_ui_handle++;
    g_universal_containers[handle] = card;
    return handle;
}

bool trust_ui_add_split_view(uint32_t container_handle, int ratio_left, int ratio_right, uint32_t *out_left, uint32_t *out_right) {
    auto it = g_universal_containers.find(container_handle);
    if (it == g_universal_containers.end() || !it->second) return false;

    QWidget *parentWidget = it->second.data();
    QLayout *parentLayout = parentWidget->layout();
    if (!parentLayout) return false;

    QWidget *splitHost = new QWidget(parentWidget);
    QHBoxLayout *hLayout = new QHBoxLayout(splitHost);
    hLayout->setContentsMargins(0, 0, 0, 0);
    hLayout->setSpacing(8);

    QWidget *leftPanel = new QWidget(splitHost);
    QVBoxLayout *leftLayout = new QVBoxLayout(leftPanel);
    leftLayout->setContentsMargins(0, 0, 0, 0);
    leftLayout->setSpacing(6);

    QWidget *rightPanel = new QWidget(splitHost);
    QVBoxLayout *rightLayout = new QVBoxLayout(rightPanel);
    rightLayout->setContentsMargins(0, 0, 0, 0);
    rightLayout->setSpacing(6);

    hLayout->addWidget(leftPanel, ratio_left > 0 ? ratio_left : 1);
    hLayout->addWidget(rightPanel, ratio_right > 0 ? ratio_right : 1);
    parentLayout->addWidget(splitHost);

    uint32_t handle_l = g_next_ui_handle++;
    uint32_t handle_r = g_next_ui_handle++;
    g_universal_containers[handle_l] = leftPanel;
    g_universal_containers[handle_r] = rightPanel;

    if (out_left) *out_left = handle_l;
    if (out_right) *out_right = handle_r;
    return true;
}

void trust_ui_clear_container(uint32_t container_handle) {
    auto it = g_universal_containers.find(container_handle);
    if (it != g_universal_containers.end() && it->second) {
        QWidget *w = it->second.data();
        if (w && w->layout()) {
            QLayoutItem *child;
            while ((child = w->layout()->takeAt(0)) != nullptr) {
                if (child->widget()) {
                    child->widget()->hide();
                    child->widget()->deleteLater();
                }
                delete child;
            }
        }
    }
}

uint32_t trust_ui_add_combobox(uint32_t container_handle, const char *label, const char *items_csv, int initial_idx, LuaComboboxCallback callback, void *user_data) {
    auto it = g_universal_containers.find(container_handle);
    if (it == g_universal_containers.end() || !it->second) return 0;

    QWidget *parentWidget = it->second.data();
    QLayout *parentLayout = parentWidget->layout();
    if (!parentLayout) return 0;

    QWidget *rowWidget = new QWidget(parentWidget);
    QHBoxLayout *row = new QHBoxLayout(rowWidget);
    row->setContentsMargins(0, 0, 0, 0);
    row->setSpacing(8);

    QLabel *lbl = new QLabel(QString::fromUtf8(label ? label : "Select:"), rowWidget);
    lbl->setStyleSheet("font-weight: bold; color: #94a3b8; font-size: 11px;");
    QComboBox *combo = new QComboBox(rowWidget);
    combo->setStyleSheet("QComboBox { background: #0f172a; color: #f8fafc; border: 1px solid #334155; border-radius: 4px; padding: 4px 8px; font-size: 11px; }");

    if (items_csv && *items_csv) {
        QStringList items = QString::fromUtf8(items_csv).split(',');
        for (const QString &item : items) combo->addItem(item.trimmed());
    }
    if (initial_idx >= 0 && initial_idx < combo->count()) combo->setCurrentIndex(initial_idx);

    row->addWidget(lbl);
    row->addWidget(combo, 1);
    parentLayout->addWidget(rowWidget);

    uint32_t handle = g_next_ui_handle++;
    DynamicWidgetNode node;
    node.handle = handle;
    node.widget = combo;
    node.userData = user_data;
    g_dynamic_widgets[handle] = node;

    QObject::connect(combo, QOverload<int>::of(&QComboBox::currentIndexChanged), [callback, combo, user_data](int idx) {
        if (callback) callback(idx, combo->currentText().toUtf8().constData(), user_data);
    });

    return handle;
}

uint32_t trust_ui_add_button(uint32_t container_handle, const char *label, const char *style_type, LuaBtnCallback callback, void *user_data) {
    auto it = g_universal_containers.find(container_handle);
    if (it == g_universal_containers.end() || !it->second) return 0;

    QWidget *parentWidget = it->second.data();
    QLayout *parentLayout = parentWidget->layout();
    if (!parentLayout) return 0;

    QPushButton *btn = new QPushButton(QString::fromUtf8(label ? label : "Button"), parentWidget);
    // No setStyleSheet — a plain QPushButton picks up OBS's own theme
    // (colors, hover/pressed states) automatically, same as every native
    // OBS button; a custom style here would fight that instead of matching it.

    parentLayout->addWidget(btn);

    uint32_t handle = g_next_ui_handle++;
    DynamicWidgetNode node;
    node.handle = handle;
    node.widget = btn;
    QObject::connect(btn, &QPushButton::clicked, [callback, user_data]() {
        if (callback) {
            QTimer::singleShot(0, [callback, user_data]() {
                if (callback) callback(user_data);
            });
        }
    });

    return handle;
}

uint32_t trust_ui_add_checkbox(uint32_t container_handle, const char *label, bool initial_state, LuaCheckCallback callback, void *user_data) {
    auto it = g_universal_containers.find(container_handle);
    if (it == g_universal_containers.end() || !it->second) return 0;

    QWidget *parentWidget = it->second.data();
    QLayout *parentLayout = parentWidget->layout();
    if (!parentLayout) return 0;

    QCheckBox *check = new QCheckBox(QString::fromUtf8(label ? label : "Option"), parentWidget);
    check->setChecked(initial_state);
    check->setStyleSheet("color: #e2e8f0; font-size: 11px;");
    parentLayout->addWidget(check);

    uint32_t handle = g_next_ui_handle++;
    DynamicWidgetNode node;
    node.handle = handle;
    node.widget = check;
    node.checkCb = callback;
    node.userData = user_data;
    g_dynamic_widgets[handle] = node;

    QObject::connect(check, &QCheckBox::toggled, [callback, user_data](bool checked) {
        if (callback) callback(checked, user_data);
    });

    return handle;
}

uint32_t trust_ui_add_table(uint32_t container_handle, const char *headers_csv) {
    auto it = g_universal_containers.find(container_handle);
    if (it == g_universal_containers.end() || !it->second) return 0;

    QWidget *parentWidget = it->second.data();
    QLayout *parentLayout = parentWidget->layout();
    if (!parentLayout) return 0;

    QTableWidget *table = new QTableWidget(parentWidget);
    table->setShowGrid(true);
    table->setStyleSheet(
        "QTableWidget { background-color: #0b1120; alternate-background-color: #1e293b; color: #f8fafc; gridline-color: #334155; border: 1px solid #334155; border-radius: 6px; font-size: 13px; }"
        "QHeaderView { background-color: #1e293b; border: none; margin: 0px; padding: 0px; }"
        "QHeaderView::section { background-color: #1e293b; color: #38bdf8; font-weight: bold; border: none; border-right: 1px solid #334155; border-bottom: 1px solid #334155; margin: 0px; padding: 0px 4px; height: 32px; font-size: 12px; }"
        "QHeaderView::section:last { border-right: none; }"
        "QTableWidget::item { padding: 4px 8px; border: none; }"
        "QTableWidget::item:selected { background-color: #0284c7; color: #ffffff; }"
    );
    table->setAlternatingRowColors(true);
    table->verticalHeader()->setVisible(false);
    table->horizontalHeader()->setHighlightSections(false);
    table->horizontalHeader()->setDefaultAlignment(Qt::AlignCenter);
    table->setSelectionBehavior(QAbstractItemView::SelectRows);

    QStringList headers;
    if (headers_csv && *headers_csv) {
        for (const QString &h : QString::fromUtf8(headers_csv).split(',')) {
            headers << h.trimmed();
        }
    }
    table->setColumnCount(headers.size());
    table->setHorizontalHeaderLabels(headers);
    if (headers.size() >= 3) {
        if (table->horizontalHeaderItem(0)) table->horizontalHeaderItem(0)->setTextAlignment(Qt::AlignLeft | Qt::AlignVCenter);
        if (table->horizontalHeaderItem(1)) table->horizontalHeaderItem(1)->setTextAlignment(Qt::AlignCenter);
        if (table->horizontalHeaderItem(2)) table->horizontalHeaderItem(2)->setTextAlignment(Qt::AlignCenter);
        table->horizontalHeader()->setSectionResizeMode(0, QHeaderView::Stretch);
        table->horizontalHeader()->setSectionResizeMode(1, QHeaderView::Fixed);
        table->horizontalHeader()->setSectionResizeMode(2, QHeaderView::Fixed);
        table->horizontalHeader()->resizeSection(1, 80);
        table->horizontalHeader()->resizeSection(2, 95);
    }

    parentLayout->addWidget(table);

    uint32_t handle = g_next_ui_handle++;
    DynamicWidgetNode node;
    node.handle = handle;
    node.widget = table;
    g_dynamic_widgets[handle] = node;
    return handle;
}

void trust_ui_add_table_row(uint32_t table_handle, const char *name, bool target_checked, bool custom_checked, LuaTableCheckCallback callback, void *user_data) {
    auto it = g_dynamic_widgets.find(table_handle);
    if (it == g_dynamic_widgets.end() || !it->second.widget) return;

    QTableWidget *table = qobject_cast<QTableWidget*>(it->second.widget.data());
    if (!table) return;

    int row = table->rowCount();
    table->insertRow(row);
    table->setRowHeight(row, 34);

    QTableWidgetItem *nameItem = new QTableWidgetItem(QString::fromUtf8(name ? name : ""));
    nameItem->setFlags(nameItem->flags() & ~Qt::ItemIsEditable);
    nameItem->setTextAlignment(Qt::AlignLeft | Qt::AlignVCenter);
    nameItem->setForeground(QBrush(QColor("#f8fafc")));
    table->setItem(row, 0, nameItem);

    QWidget *checkWidget1 = new QWidget(table);
    checkWidget1->setStyleSheet("background: transparent; margin: 0px; padding: 0px;");
    QHBoxLayout *layout1 = new QHBoxLayout(checkWidget1);
    layout1->setContentsMargins(0, 0, 0, 0);
    layout1->setSpacing(0);
    layout1->setAlignment(Qt::AlignCenter);
    QCheckBox *cb1 = new QCheckBox(checkWidget1);
    cb1->setChecked(target_checked);
    layout1->addWidget(cb1);
    table->setCellWidget(row, 1, checkWidget1);

    QObject::connect(cb1, &QCheckBox::toggled, [callback, row, user_data](bool checked) {
        if (callback) callback(row, 1, checked, user_data);
    });

    QWidget *checkWidget2 = new QWidget(table);
    checkWidget2->setStyleSheet("background: transparent; margin: 0px; padding: 0px;");
    QHBoxLayout *layout2 = new QHBoxLayout(checkWidget2);
    layout2->setContentsMargins(0, 0, 0, 0);
    layout2->setSpacing(0);
    layout2->setAlignment(Qt::AlignCenter);
    QCheckBox *cb2 = new QCheckBox(checkWidget2);
    cb2->setChecked(custom_checked);
    layout2->addWidget(cb2);
    table->setCellWidget(row, 2, checkWidget2);

    QObject::connect(cb2, &QCheckBox::toggled, [callback, row, user_data](bool checked) {
        if (callback) callback(row, 2, checked, user_data);
    });
}

void trust_ui_clear_table(uint32_t table_handle) {
    auto it = g_dynamic_widgets.find(table_handle);
    if (it != g_dynamic_widgets.end() && it->second.widget) {
        if (QTableWidget *table = qobject_cast<QTableWidget*>(it->second.widget.data())) {
            table->setRowCount(0);
        }
    }
}

void trust_ui_set_table_select_callback(uint32_t table_handle, LuaTableSelectCallback callback, void *user_data) {
    auto it = g_dynamic_widgets.find(table_handle);
    if (it == g_dynamic_widgets.end() || !it->second.widget) return;

    QTableWidget *table = qobject_cast<QTableWidget*>(it->second.widget.data());
    if (!table) return;

    QObject::connect(table, &QTableWidget::itemSelectionChanged, [table, callback, user_data]() {
        int r = table->currentRow();
        if (r >= 0 && r < table->rowCount()) {
            QTableWidgetItem *item = table->item(r, 0);
            if (item && callback) {
                std::string s = item->text().toStdString();
                callback(r, s.c_str(), user_data);
            }
        }
    });
}

void trust_ui_show_window(uint32_t window_handle) {
    auto it = g_universal_windows.find(window_handle);
    if (it != g_universal_windows.end() && it->second) {
        QDialog *win = it->second.data();
        if (win) {
            win->show();
            win->raise();
            win->activateWindow();
        }
    }
}

void trust_ui_show_dock(uint32_t dock_handle) {
    auto it = g_universal_docks.find(dock_handle);
    QDockWidget *dock = (it != g_universal_docks.end() && it->second) ? it->second.data() : nullptr;

    // Docks created via obs_frontend_add_dock_by_id only have their plain
    // content widget in g_universal_containers — OBS owns the actual
    // QDockWidget wrapper. Walk up the parent chain to find it.
    if (!dock) {
        auto cit = g_universal_containers.find(dock_handle);
        if (cit != g_universal_containers.end() && cit->second) {
            QWidget *w = cit->second.data()->parentWidget();
            while (w && !qobject_cast<QDockWidget*>(w)) {
                w = w->parentWidget();
            }
            dock = qobject_cast<QDockWidget*>(w);
        }
    }

    if (dock) {
        dock->show();
        dock->raise();
        dock->activateWindow();
        if (dock->toggleViewAction()) {
            dock->toggleViewAction()->setChecked(true);
        }
    }
}

}

// ============================================================================
// 🧹 Cleanup — call from the host's own module-unload path (e.g. Rust's
// obs_module_unload in lib.rs). Not an OBS module hook itself; this file is
// a plain utility library, not the OBS-loaded module.
// ============================================================================
extern "C" {

void trust_ui_cleanup(void) {
    for (auto &pair : g_dynamic_widgets) {
        pair.second.sliderCb = nullptr;
        pair.second.btnCb = nullptr;
        pair.second.textCb = nullptr;
        pair.second.checkCb = nullptr;
        if (pair.second.widget) {
            pair.second.widget->disconnect();
        }
    }
    g_dynamic_widgets.clear();
    g_universal_containers.clear();
    g_universal_windows.clear();
    g_universal_docks.clear();

    if (g_top_menu) {
        g_top_menu->disconnect();
        g_top_menu = nullptr;
    }
    printf("[Theater Plugin] 🛑 Qt UI toolkit cleaned up.\n");
    fflush(stdout);
}

}
