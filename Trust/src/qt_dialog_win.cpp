#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <cstdint>
#include <cstdio>
#include <cstdarg>
#include <unordered_map>
#include <string>
#include <vector>

// ============================================================================
// File Logging Helper for Debugging
// ============================================================================
static void qt_log(const char *fmt, ...) {
    char buf[1024];
    va_list args;
    va_start(args, fmt);
    vsnprintf(buf, sizeof(buf), fmt, args);
    va_end(args);

    OutputDebugStringA(buf);

    char appdata[MAX_PATH];
    if (GetEnvironmentVariableA("APPDATA", appdata, MAX_PATH) > 0) {
        char logpath[MAX_PATH];
        snprintf(logpath, sizeof(logpath), "%s\\obs-studio\\logs\\trust_qt.log", appdata);
        FILE *f = fopen(logpath, "a");
        if (f) {
            SYSTEMTIME st;
            GetLocalTime(&st);
            fprintf(f, "[%02d:%02d:%02d.%03d] %s\n", st.wHour, st.wMinute, st.wSecond, st.wMilliseconds, buf);
            fclose(f);
        }
    }
}

// ============================================================================
// Forward Declarations of Qt Internal Types
// ============================================================================
class QEvent;
class QTimerEvent;
class QChildEvent;
class QMetaMethod;

template <typename Enum>
class QFlags {
    int i;
public:
    QFlags(int val = 0) : i(val) {}
    int toInt() const { return i; }
};

struct QMetaObject {
    enum Call {
        InvokeMetaMethod,
        ReadProperty,
        WriteProperty,
        ResetProperty,
        CreateInstance,
        IndexOfMethod,
        RegisterPropertyMetaType,
        RegisterMethodArgumentMetaType,
        BindableProperty,
        CustomCall
    };
    class Connection {
        void *d_ptr;
    };
};

namespace Qt {
    enum ConnectionType {
        AutoConnection,
        DirectConnection,
        QueuedConnection,
        BlockingQueuedConnection,
        UniqueConnection = 0x80
    };
    enum WindowType {
        Widget = 0x00000000,
        Window = 0x00000001
    };
    enum AlignmentFlag {
        AlignLeading = 0x0001,
        AlignLeft = 0x0001,
        AlignHCenter = 0x0004,
        AlignVCenter = 0x0080,
        AlignCenter = AlignVCenter | AlignHCenter
    };
    enum ItemFlag {
        ItemIsSelectable = 1,
        ItemIsEditable = 2,
        ItemIsDragEnabled = 4,
        ItemIsDropEnabled = 8,
        ItemIsUserCheckable = 16,
        ItemIsEnabled = 32
    };
    enum BrushStyle {
        NoBrush,
        SolidPattern
    };
    enum FindChildOption {
        FindDirectChildrenOnly = 0x0,
        FindChildrenRecursively = 0x1
    };
}

// ============================================================================
// Qt Classes imported from Qt6Core.dll, Qt6Gui.dll, Qt6Widgets.dll
// ============================================================================
class __declspec(dllimport) QString {
public:
    void *d;
    void *ptr;
    size_t size;
    QString();
    ~QString();
    static QString fromUtf8(const char *str, int64_t size = -1);
};

static_assert(sizeof(QString) == 24, "QString size mismatch");

class __declspec(dllimport) QObject {
public:
    void *vptr;
    void *d_ptr;
    QObject(QObject *parent = nullptr);
    void setObjectName(const QString &name);
    void installEventFilter(QObject *filterObj);
    void deleteLater();
    bool inherits(const char *classname) const;
    static bool disconnect(const QObject *sender, const char *signal, const QObject *receiver, const char *member);
};

// A native window Qt didn't create (QWindow::fromWinId): Theater's egui docks
class __declspec(dllimport) QWindow : public QObject {
public:
    static QWindow *fromWinId(unsigned long long id);
    void setParent(QWindow *parent);
    void hide();
};

class __declspec(dllimport) QIcon {
public:
    void *d;
    QIcon(const QString &fileName);
    ~QIcon();
};

static_assert(sizeof(QIcon) == 8, "QIcon size mismatch");

class __declspec(dllimport) QAction : public QObject {
public:
    void setChecked(bool);
    void setIcon(const QIcon &icon);
};

class __declspec(dllimport) QLayout;

// QWidget in 64-bit Qt6: QObject (16 bytes) + QPaintDevice (16 bytes) + QWidgetData* (8 bytes) = 40 bytes
class __declspec(dllimport) QWidget : public QObject {
public:
    char _pad_paintdevice_and_data[24];
    QWidget(QWidget *parent = nullptr, QFlags<Qt::WindowType> flags = {});
    QLayout* layout() const;
    QWidget* parentWidget() const;
    void show();
    void raise();
    void activateWindow();
    void removeAction(QAction *action);
    void update();
    void hide();
    void resize(int w, int h);
    void setWindowTitle(const QString &title);
    void setStyleSheet(const QString &styleSheet);
    static QWidget *createWindowContainer(QWindow *window, QWidget *parent = nullptr, QFlags<Qt::WindowType> flags = {});
};

// QLayoutItem in 64-bit Qt6: vptr (8 bytes) + Qt::Alignment (4 bytes, padded) = 16 bytes
class __declspec(dllimport) QLayoutItem {
public:
    void *vptr_layoutitem;
    int align;
    void setAlignment(QFlags<Qt::AlignmentFlag> alignment);
};

// QLayout in 64-bit Qt6: QObject (16 bytes) + QLayoutItem (16 bytes) = 32 bytes
class __declspec(dllimport) QLayout : public QObject, public QLayoutItem {
public:
    void setContentsMargins(int left, int top, int right, int bottom);
    void addWidget(QWidget *w);
};

class __declspec(dllimport) QBoxLayout : public QLayout {
public:
    void addWidget(QWidget *w, int stretch = 0, QFlags<Qt::AlignmentFlag> alignment = {});
};

class __declspec(dllimport) QVBoxLayout : public QBoxLayout {
public:
    QVBoxLayout(QWidget *parent);
};

class __declspec(dllimport) QHBoxLayout : public QBoxLayout {
public:
    QHBoxLayout(QWidget *parent);
};

class __declspec(dllimport) QAbstractButton : public QWidget {
public:
    void setChecked(bool);
};

class __declspec(dllimport) QPushButton : public QAbstractButton {
public:
    QPushButton(const QString &text, QWidget *parent = nullptr);
};

class __declspec(dllimport) QCheckBox : public QAbstractButton {
public:
    QCheckBox(QWidget *parent = nullptr);
    QCheckBox(const QString &text, QWidget *parent = nullptr);
};

class __declspec(dllimport) QMenu : public QWidget {
public:
    void setTitle(const QString &title);
    void clear();
    QAction* addAction(const QString &text);
    QAction* addSeparator();
};

class __declspec(dllimport) QMenuBar : public QWidget {
public:
    QMenu* addMenu(const QString &title);
};

class __declspec(dllimport) QDockWidget : public QWidget {
public:
    QDockWidget(QWidget *parent = nullptr, QFlags<Qt::WindowType> flags = {});
    void setWidget(QWidget *widget);
    void setFloating(bool floating);
};

class __declspec(dllimport) QMainWindow : public QWidget {
public:
    QMenuBar* menuBar() const;
};

class __declspec(dllimport) QDialog : public QWidget {
public:
    QDialog(QWidget *parent = nullptr, QFlags<Qt::WindowType> flags = {});
};

class __declspec(dllimport) QFrame : public QWidget {
public:
    QFrame(QWidget *parent = nullptr, QFlags<Qt::WindowType> flags = {});
};

class __declspec(dllimport) QLabel : public QFrame {
public:
    QLabel(const QString &text, QWidget *parent = nullptr, QFlags<Qt::WindowType> flags = {});
};

class __declspec(dllimport) QAbstractScrollArea : public QFrame {};

class __declspec(dllimport) QScrollArea : public QAbstractScrollArea {
public:
    QScrollArea(QWidget *parent = nullptr);
    void setWidget(QWidget *widget);
    void setWidgetResizable(bool resizable);
};

class __declspec(dllimport) QTabWidget : public QWidget {
public:
    QTabWidget(QWidget *parent = nullptr);
    int addTab(QWidget *widget, const QString &label);
};

// QVariant in 64-bit Qt6: 24 bytes inline storage + packed type/flags word = 32 bytes
class __declspec(dllimport) QVariant {
public:
    void *data[3];
    uintptr_t packed;
    QVariant();
    ~QVariant();
};

class __declspec(dllimport) QComboBox : public QWidget {
public:
    QComboBox(QWidget *parent = nullptr);
    void addItem(const QString &text, const QVariant &userData = QVariant());
    int count() const;
    void setCurrentIndex(int index);
    QString currentText() const;
};

// QColor in 64-bit Qt6: Spec (4 bytes) + 5 ushort components (10 bytes), padded = 16 bytes
class __declspec(dllimport) QColor {
public:
    int cspec;
    unsigned short ct[5];
    QColor(const char *name);
};

// QBrush in 64-bit Qt6: single QBrushData pointer = 8 bytes
class __declspec(dllimport) QBrush {
public:
    void *d;
    QBrush(const QColor &color, Qt::BrushStyle style = Qt::SolidPattern);
    ~QBrush();
};

// QTableWidgetItem in 64-bit Qt6 (not a QObject): vptr + rtti + QList values + view + d + itemFlags = 64 bytes
class __declspec(dllimport) QTableWidgetItem {
public:
    void *vptr;
    char _pad_item_data[56];
    QTableWidgetItem(const QString &text, int type = 0);
    QFlags<Qt::ItemFlag> flags() const;
    void setFlags(QFlags<Qt::ItemFlag> flags);
    void setTextAlignment(Qt::AlignmentFlag alignment);
    void setTextAlignment(QFlags<Qt::AlignmentFlag> alignment);
    void setForeground(const QBrush &brush);
    QString text() const;
};

class __declspec(dllimport) QAbstractItemView : public QAbstractScrollArea {
public:
    enum SelectionBehavior { SelectItems, SelectRows, SelectColumns };
    void setAlternatingRowColors(bool enable);
    void setSelectionBehavior(SelectionBehavior behavior);
};

class __declspec(dllimport) QHeaderView : public QAbstractItemView {
public:
    enum ResizeMode { Interactive, Stretch, Fixed, ResizeToContents, Custom = Fixed };
    void setHighlightSections(bool highlight);
    void setDefaultAlignment(QFlags<Qt::AlignmentFlag> alignment);
    void setSectionResizeMode(int logicalIndex, ResizeMode mode);
    void resizeSection(int logicalIndex, int size);
};

class __declspec(dllimport) QTableView : public QAbstractItemView {
public:
    QHeaderView* horizontalHeader() const;
    QHeaderView* verticalHeader() const;
    void setShowGrid(bool show);
    void setRowHeight(int row, int height);
};

class __declspec(dllimport) QTableWidget : public QTableView {
public:
    QTableWidget(QWidget *parent = nullptr);
    int rowCount() const;
    void setRowCount(int rows);
    void setColumnCount(int columns);
    void insertRow(int row);
    int currentRow() const;
    QTableWidgetItem* item(int row, int column) const;
    void setItem(int row, int column, QTableWidgetItem *item);
    QTableWidgetItem* horizontalHeaderItem(int column) const;
    void setHorizontalHeaderItem(int column, QTableWidgetItem *item);
    void setCellWidget(int row, int column, QWidget *widget);
};

// Backs QObject::findChild<T>(), which is an inline template in the real Qt headers
__declspec(dllimport) QObject* qt_qFindChild_helper(const QObject *parent, const QString &name, const QMetaObject &mo, QFlags<Qt::FindChildOption> options);

static_assert(sizeof(QObject) == 16, "QObject size mismatch");
static_assert(sizeof(QWidget) == 40, "QWidget size mismatch");
static_assert(sizeof(QLayout) == 32, "QLayout size mismatch");
static_assert(sizeof(QVBoxLayout) == 32, "QVBoxLayout size mismatch");
static_assert(sizeof(QPushButton) == 40, "QPushButton size mismatch");
static_assert(sizeof(QLayoutItem) == 16, "QLayoutItem size mismatch");
static_assert(sizeof(QHBoxLayout) == 32, "QHBoxLayout size mismatch");
static_assert(sizeof(QCheckBox) == 40, "QCheckBox size mismatch");
static_assert(sizeof(QDialog) == 40, "QDialog size mismatch");
static_assert(sizeof(QFrame) == 40, "QFrame size mismatch");
static_assert(sizeof(QLabel) == 40, "QLabel size mismatch");
static_assert(sizeof(QScrollArea) == 40, "QScrollArea size mismatch");
static_assert(sizeof(QTabWidget) == 40, "QTabWidget size mismatch");
static_assert(sizeof(QComboBox) == 40, "QComboBox size mismatch");
static_assert(sizeof(QTableWidget) == 40, "QTableWidget size mismatch");
static_assert(sizeof(QTableWidgetItem) == 64, "QTableWidgetItem size mismatch");
static_assert(sizeof(QVariant) == 32, "QVariant size mismatch");
static_assert(sizeof(QColor) == 16, "QColor size mismatch");
static_assert(sizeof(QBrush) == 8, "QBrush size mismatch");

// ============================================================================
// QPointer equivalent (same semantics as the Linux build's QPointer<T>)
// Qt6's QPointer is a weak reference onto Qt's own refcount block
// (QtSharedPointer::ExternalRefCountData), whose strongref Qt zeroes in
// ~QObject — so a stored handle reads back as nullptr once Qt deletes the
// widget (e.g. after trust_ui_clear_container) instead of dangling.
// ============================================================================
struct QtWeakRefData {
    volatile long weakref;
    volatile long strongref;
    void *destroyer;
};

typedef QtWeakRefData* (*GetAndRefFn)(const QObject *obj);

template <typename T>
class QPointer {
    QtWeakRefData *d = nullptr;
    T *value = nullptr;

    void acquire(T *p) {
        value = p;
        if (!p) return;
        HMODULE qtCore = GetModuleHandleA("Qt6Core.dll");
        if (!qtCore) qtCore = LoadLibraryA("Qt6Core.dll");
        if (!qtCore) return;
        static auto get_and_ref_fn = (GetAndRefFn)GetProcAddress(qtCore, "?getAndRef@ExternalRefCountData@QtSharedPointer@@SAPEAU12@PEBVQObject@@@Z");
        if (get_and_ref_fn) d = get_and_ref_fn((const QObject*)p);
    }
    void release() {
        if (d && InterlockedDecrement(&d->weakref) == 0) ::operator delete(d);
        d = nullptr;
        value = nullptr;
    }

public:
    QPointer() = default;
    QPointer(T *p) { acquire(p); }
    QPointer(const QPointer &other) : d(other.d), value(other.value) {
        if (d) InterlockedIncrement(&d->weakref);
    }
    ~QPointer() { release(); }
    QPointer &operator=(const QPointer &other) {
        if (this != &other) {
            if (other.d) InterlockedIncrement(&other.d->weakref);
            QtWeakRefData *newD = other.d;
            T *newValue = other.value;
            release();
            d = newD;
            value = newValue;
        }
        return *this;
    }
    QPointer &operator=(T *p) {
        release();
        acquire(p);
        return *this;
    }
    // Without getAndRef (d stays null) this degrades to a plain pointer.
    T* data() const { return (d && d->strongref == 0) ? nullptr : value; }
    operator T*() const { return data(); }
    T* operator->() const { return data(); }
};

// ============================================================================
// Qt6 Internal Signal/Slot Functor Connection (QSlotObjectBase)
// ============================================================================
typedef void (*LuaBtnCallback)(void *user_data);
typedef void (*LuaSliderCallback)(double value, void *user_data);
typedef void (*LuaTextCallback)(const char *text, void *user_data);
typedef void (*LuaCheckCallback)(bool checked, void *user_data);
typedef void (*LuaComboboxCallback)(int selected_idx, const char *selected_text, void *user_data);
typedef void (*LuaTableCheckCallback)(int row, int col, bool checked, void *user_data);
typedef void (*LuaTableSelectCallback)(int row, const char *name, void *user_data);

namespace QtPrivate {
    struct QSlotObjectBase;
    typedef void (*QSlotFunc)(int which, QSlotObjectBase *this_, QObject *r, void **a, bool *ret);

    struct QSlotObjectBase {
        int ref;
        int pad;
        QSlotFunc m_impl;
        QSlotObjectBase(QSlotFunc impl) : ref(1), pad(0), m_impl(impl) {}
    };

    struct CustomSlotObject : public QSlotObjectBase {
        LuaBtnCallback cb;
        void *user_data;
        CustomSlotObject(LuaBtnCallback cb, void *user_data)
            : QSlotObjectBase(impl), cb(cb), user_data(user_data) {}

        static void impl(int which, QSlotObjectBase *this_, QObject *r, void **a, bool *ret) {
            CustomSlotObject *self = (CustomSlotObject*)this_;
            if (which == 0) { // Destroy
                delete self;
            } else if (which == 1) { // Call
                if (self && self->cb) {
                    self->cb(self->user_data);
                }
            } else if (which == 2) { // Compare
                if (ret) *ret = false;
            }
        }
    };

    // Same mechanism for signals that carry arguments (toggled(bool),
    // currentIndexChanged(int), ...): a[1..n] point at the signal's
    // arguments, and `dispatch` unpacks them for the specific Lua callback.
    struct ArgSlotObject;
    typedef void (*ArgSlotDispatch)(ArgSlotObject *self, void **a);

    struct ArgSlotObject : public QSlotObjectBase {
        ArgSlotDispatch dispatch;
        void *cb;
        void *user_data;
        QObject *context;
        int row;
        int col;
        ArgSlotObject(ArgSlotDispatch dispatch, void *cb, void *user_data, QObject *context = nullptr, int row = 0, int col = 0)
            : QSlotObjectBase(impl), dispatch(dispatch), cb(cb), user_data(user_data), context(context), row(row), col(col) {}

        static void impl(int which, QSlotObjectBase *this_, QObject *r, void **a, bool *ret) {
            ArgSlotObject *self = (ArgSlotObject*)this_;
            if (which == 0) { // Destroy
                delete self;
            } else if (which == 1) { // Call
                if (self && self->cb && self->dispatch) {
                    self->dispatch(self, a);
                }
            } else if (which == 2) { // Compare
                if (ret) *ret = false;
            }
        }
    };
}

typedef void* (*QObjectPrivateGetFn)(const QObject *o);
typedef int (*SignalIndexFn)(const void *priv, const char *signalName, const void **meta);
typedef void* (*QObjectPrivateConnectFn)(void *out_connection, const QObject *sender, int signal_index, void *slot, int type);
typedef void (*ConnectionDtorFn)(void *connection);
typedef int (*IndexOfSignalFn)(const void *meta, const char *signal);

// Takes ownership of `slot` (destroyed here if the connection can't be made).
static bool connect_signal_to_slot(QObject *sender, const char *sigName1, const char *sigName2, QtPrivate::QSlotObjectBase *slot) {
    auto destroy_slot = [slot]() { slot->m_impl(0, slot, nullptr, nullptr, nullptr); };

    HMODULE qtCore = GetModuleHandleA("Qt6Core.dll");
    if (!qtCore) qtCore = LoadLibraryA("Qt6Core.dll");
    if (!qtCore) { destroy_slot(); return false; }

    static auto get_fn = (QObjectPrivateGetFn)GetProcAddress(qtCore, "?get@QObjectPrivate@@SAPEAV1@PEAVQObject@@@Z");
    static auto sig_fn = (SignalIndexFn)GetProcAddress(qtCore, "?signalIndex@QObjectPrivate@@QEBAHPEBDPEAPEBUQMetaObject@@@Z");
    static auto conn_fn = (QObjectPrivateConnectFn)GetProcAddress(qtCore, "?connect@QObjectPrivate@@SA?AVConnection@QMetaObject@@PEBVQObject@@HPEAVQSlotObjectBase@QtPrivate@@W4ConnectionType@Qt@@@Z");
    static auto conn_dtor_fn = (ConnectionDtorFn)GetProcAddress(qtCore, "??1Connection@QMetaObject@@QEAA@XZ");
    static auto index_of_signal_fn = (IndexOfSignalFn)GetProcAddress(qtCore, "?indexOfSignal@QMetaObject@@QEBAHPEBD@Z");

    if (!get_fn || !sig_fn || !conn_fn || !index_of_signal_fn) { destroy_slot(); return false; }

    void *priv = get_fn(sender);
    if (!priv) { destroy_slot(); return false; }

    // signalIndex() locates the signal and the class declaring it (`meta`), but
    // returns a *signal* index; QObjectPrivate::connect() takes the signal's
    // *method* index and converts it to a signal index itself.
    const char *sigName = nullptr;
    const void *meta = nullptr;
    if (sigName1 && sig_fn(priv, sigName1, &meta) >= 0) sigName = sigName1;
    else if (sigName2 && sig_fn(priv, sigName2, &meta) >= 0) sigName = sigName2;
    int methodIdx = (sigName && meta) ? index_of_signal_fn(meta, sigName) : -1;
    if (methodIdx < 0) { destroy_slot(); return false; }

    char dummy_conn[32] = {0};
    conn_fn(dummy_conn, sender, methodIdx, slot, 0); // 0 = Qt::AutoConnection
    // Release the returned QMetaObject::Connection handle (the connection itself stays alive)
    if (conn_dtor_fn) conn_dtor_fn(dummy_conn);
    return true;
}

static bool connect_signal_to_callback(QObject *sender, const char *sigName1, const char *sigName2, LuaBtnCallback cb, void *user_data) {
    return connect_signal_to_slot(sender, sigName1, sigName2, new QtPrivate::CustomSlotObject(cb, user_data));
}

// ============================================================================
// Header-free stand-ins for inline Qt helpers and virtual Qt methods
// ============================================================================

// Virtual Qt methods can't be declared in the dllimport classes above (a
// `virtual` there would give those classes a vtable of their own), so their
// concrete implementations are resolved by mangled name, same as the
// QObjectPrivate symbols. Every layout this file creates is a QVBoxLayout or
// QHBoxLayout, neither of which overrides these QBoxLayout methods.
static FARPROC qt_widgets_proc(const char *symbol) {
    HMODULE qtWidgets = GetModuleHandleA("Qt6Widgets.dll");
    if (!qtWidgets) qtWidgets = LoadLibraryA("Qt6Widgets.dll");
    FARPROC fn = qtWidgets ? GetProcAddress(qtWidgets, symbol) : nullptr;
    if (!fn) qt_log("[trust_qt] ⚠️ Qt6Widgets symbol not found: %s", symbol);
    return fn;
}

static void box_set_spacing(QBoxLayout *layout, int spacing) {
    typedef void (*Fn)(QBoxLayout *self, int spacing);
    static auto fn = (Fn)qt_widgets_proc("?setSpacing@QBoxLayout@@UEAAXH@Z");
    if (fn && layout) fn(layout, spacing);
}

static QLayoutItem* box_take_at(QBoxLayout *layout, int index) {
    typedef QLayoutItem* (*Fn)(QBoxLayout *self, int index);
    static auto fn = (Fn)qt_widgets_proc("?takeAt@QBoxLayout@@UEAAPEAVQLayoutItem@@H@Z");
    return (fn && layout) ? fn(layout, index) : nullptr;
}

// Items in these layouts only ever come from addWidget(), i.e. QWidgetItem(V2)
static QWidget* layout_item_widget(QLayoutItem *item) {
    typedef QWidget* (*Fn)(QLayoutItem *self);
    static auto fn = (Fn)qt_widgets_proc("?widget@QWidgetItem@@UEBAPEAVQWidget@@XZ");
    return (fn && item) ? fn(item) : nullptr;
}

// `delete item` — QLayoutItem declares its virtual destructor first, so vtable
// slot 0 is the (vector) deleting destructor; flag 1 = delete a single object.
static void layout_item_delete(QLayoutItem *item) {
    if (!item) return;
    typedef void (*DeletingDtorFn)(QLayoutItem *self, unsigned int flags);
    DeletingDtorFn dtor = (*(DeletingDtorFn**)item)[0];
    dtor(item, 1);
}

static void header_set_visible(QHeaderView *header, bool visible) {
    typedef void (*Fn)(QHeaderView *self, bool visible);
    static auto fn = (Fn)qt_widgets_proc("?setVisible@QHeaderView@@UEAAX_N@Z");
    if (fn && header) fn(header, visible);
}

// qobject_cast<T*>() equivalent — QObject::inherits() walks the real metaobject chain
template <typename T>
static T* qt_cast(QObject *obj, const char *className) {
    return (obj && obj->inherits(className)) ? static_cast<T*>(obj) : nullptr;
}

// QString::toUtf8() equivalent — QString's ptr/size are UTF-16 data
static std::string qstring_to_utf8(const QString &s) {
    if (!s.ptr || s.size == 0) return std::string();
    int n = (int)s.size;
    int len = WideCharToMultiByte(CP_UTF8, 0, (const wchar_t*)s.ptr, n, nullptr, 0, nullptr, nullptr);
    if (len <= 0) return std::string();
    std::string out(len, '\0');
    WideCharToMultiByte(CP_UTF8, 0, (const wchar_t*)s.ptr, n, &out[0], len, nullptr, nullptr);
    return out;
}

// QString::split(',') + trimmed() equivalent (empty parts kept, like Qt's default)
static std::vector<std::string> split_csv_trimmed(const char *csv) {
    std::vector<std::string> parts;
    std::string s(csv ? csv : "");
    const char *ws = " \t\n\v\f\r";
    size_t start = 0;
    while (true) {
        size_t comma = s.find(',', start);
        std::string part = s.substr(start, comma == std::string::npos ? std::string::npos : comma - start);
        size_t b = part.find_first_not_of(ws);
        size_t e = part.find_last_not_of(ws);
        parts.push_back(b == std::string::npos ? std::string() : part.substr(b, e - b + 1));
        if (comma == std::string::npos) break;
        start = comma + 1;
    }
    return parts;
}

// ============================================================================
// Signal dispatchers (the Linux build's connect() lambdas)
// ============================================================================
static void dispatch_check(QtPrivate::ArgSlotObject *self, void **a) {
    bool checked = *(bool*)a[1];
    ((LuaCheckCallback)self->cb)(checked, self->user_data);
}

static void dispatch_combobox(QtPrivate::ArgSlotObject *self, void **a) {
    int idx = *(int*)a[1];
    QComboBox *combo = (QComboBox*)self->context;
    QString text = combo->currentText();
    std::string s = qstring_to_utf8(text);
    ((LuaComboboxCallback)self->cb)(idx, s.c_str(), self->user_data);
}

static void dispatch_table_check(QtPrivate::ArgSlotObject *self, void **a) {
    bool checked = *(bool*)a[1];
    ((LuaTableCheckCallback)self->cb)(self->row, self->col, checked, self->user_data);
}

static void dispatch_table_select(QtPrivate::ArgSlotObject *self, void **a) {
    QTableWidget *table = (QTableWidget*)self->context;
    int r = table->currentRow();
    if (r >= 0 && r < table->rowCount()) {
        QTableWidgetItem *item = table->item(r, 0);
        if (item) {
            QString text = item->text();
            std::string s = qstring_to_utf8(text);
            ((LuaTableSelectCallback)self->cb)(r, s.c_str(), self->user_data);
        }
    }
}

// ============================================================================
// Global State & OBS Helpers
// ============================================================================
struct DynamicWidgetNode {
    uint32_t handle = 0;
    QPointer<QWidget> widget;
    QPointer<QLabel> valLabel;
    double scaleFactor = 1.0;
    std::string suffix;
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
// Theater's egui docks, by the handle of the dock they're shown in
static std::unordered_map<uint32_t, QPointer<QWindow>> g_native_windows;
static QPointer<QMenu> g_top_menu;
static std::string g_current_menu_title = "🎬 Theater Scene";

static QMainWindow* find_obs_main_window() {
    HMODULE obs_fe = GetModuleHandleA("obs-frontend-api.dll");
    if (!obs_fe) obs_fe = LoadLibraryA("obs-frontend-api.dll");
    if (obs_fe) {
        typedef void* (*get_main_win_fn)();
        get_main_win_fn fn = (get_main_win_fn)GetProcAddress(obs_fe, "obs_frontend_get_main_window");
        if (fn) {
            return (QMainWindow*)fn();
        }
    }
    return nullptr;
}

// ============================================================================
// The "Theater" menu shared by all our OBS plugins: one top-level menu, found
// by object name so none of them owns it, where each plugin adds its items
// directly, in a group of its own that starts with a separator. Every item a
// plugin adds is named with the plugin's id, so it can remove its own items.
// Contract: theater-control-panel/docs/shared-menu.md. Never change these names.
// ============================================================================
static const char *kSharedMenuName = "theater_menu";
// "🎭 Theater" in UTF-8, escaped: this file is compiled without /utf-8
static const char *kSharedMenuTitle = "\xF0\x9F\x8E\xAD Theater";
static const char *kPluginName = "theater-scene";

// QMenu::staticMetaObject: a data export, resolved like QTabWidget's (see
// trust_ui_add_tab) since data can't be imported through /DELAYLOAD.
static const QMetaObject* menu_meta_object() {
    static auto meta = (const QMetaObject*)qt_widgets_proc("?staticMetaObject@QMenu@@2UQMetaObject@@B");
    return meta;
}

// QAction::staticMetaObject, the same way; QAction lives in Qt6Gui.
static const QMetaObject* action_meta_object() {
    static auto meta = []() -> const QMetaObject* {
        HMODULE qtGui = GetModuleHandleA("Qt6Gui.dll");
        if (!qtGui) qtGui = LoadLibraryA("Qt6Gui.dll");
        auto m = qtGui ? (const QMetaObject*)GetProcAddress(qtGui, "?staticMetaObject@QAction@@2UQMetaObject@@B") : nullptr;
        if (!m) qt_log("[trust_qt] ⚠️ Qt6Gui symbol not found: QAction::staticMetaObject");
        return m;
    }();
    return meta;
}

// Names `item` with a plugin's id: that's how each plugin finds its own items.
static void name_item(QAction *item, const char *plugin_id) {
    if (item) item->setObjectName(QString::fromUtf8(plugin_id));
}

static void name_own_item(QAction *item) {
    name_item(item, kPluginName);
}

// Theater Scene's icon, which its items show in the shared menu: the plugin's
// data\assets\icon_32.png, found from where this DLL is
// (...\theater-scene\bin\64bit\trust_qt.dll). Empty if it isn't there.
static const std::string& own_icon_path() {
    static const std::string path = []() -> std::string {
        HMODULE self = nullptr;
        if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                                (LPCWSTR)&own_icon_path, &self)) return {};
        wchar_t dll[MAX_PATH];
        DWORD len = GetModuleFileNameW(self, dll, MAX_PATH);
        if (len == 0 || len == MAX_PATH) return {};
        std::wstring relative(dll, len);
        relative.resize(relative.find_last_of(L'\\') + 1);
        relative += L"..\\..\\data\\assets\\icon_32.png";
        wchar_t icon[MAX_PATH];
        DWORD iconLen = GetFullPathNameW(relative.c_str(), MAX_PATH, icon, nullptr);
        if (iconLen == 0 || iconLen >= MAX_PATH || GetFileAttributesW(icon) == INVALID_FILE_ATTRIBUTES) {
            qt_log("[trust_qt] ⚠️ No menu icon at %ls", relative.c_str());
            return {};
        }
        int n = WideCharToMultiByte(CP_UTF8, 0, icon, -1, nullptr, 0, nullptr, nullptr);
        if (n <= 1) return {};
        std::string utf8(n - 1, '\0');
        WideCharToMultiByte(CP_UTF8, 0, icon, -1, &utf8[0], n, nullptr, nullptr);
        return utf8;
    }();
    return path;
}

// Removes a plugin's items (the ones named with its id) from `menu`. Its
// items are its children: addAction/addSeparator parent them to it.
static void remove_items(QMenu *menu, const char *plugin_id) {
    const QMetaObject *meta = action_meta_object();
    if (!menu || !meta || !plugin_id || !*plugin_id) return;
    QString name = QString::fromUtf8(plugin_id);
    while (QAction *item = (QAction*)qt_qFindChild_helper(menu, name, *meta, Qt::FindDirectChildrenOnly)) {
        menu->removeAction(item);
        item->setObjectName(QString());
        item->deleteLater();
    }
}

static void remove_own_items(QMenu *menu) {
    remove_items(menu, kPluginName);
}

// (Re)starts a plugin's group in `menu`: removes its items and adds the
// separator the group starts with (QMenu hides it while the group is first).
static void start_group(QMenu *menu, const char *plugin_id) {
    remove_items(menu, plugin_id);
    name_item(menu->addSeparator(), plugin_id);
}

static void start_own_group(QMenu *menu) {
    start_group(menu, kPluginName);
}

// The shared menu, created at the end of the menu bar if no plugin has yet.
static QMenu* find_shared_menu(QMenuBar *menuBar) {
    QString menuName = QString::fromUtf8(kSharedMenuName);
    const QMetaObject *meta = menu_meta_object();
    QMenu *menu = meta ? (QMenu*)qt_qFindChild_helper(menuBar, menuName, *meta, Qt::FindDirectChildrenOnly) : nullptr;
    if (!menu) {
        menu = menuBar->addMenu(QString::fromUtf8(kSharedMenuTitle));
        if (!menu) return nullptr;
        menu->setObjectName(menuName);
        qt_log("[trust_qt] Created the shared '%s' menu at %p", kSharedMenuTitle, menu);
    }
    return menu;
}

// The shared menu, with Theater Scene's group started in it.
static QMenu* get_shared_menu(QMenuBar *menuBar) {
    QMenu *menu = find_shared_menu(menuBar);
    if (menu) start_own_group(menu);
    return menu;
}

// The shared menu from OBS's main window (null before it exists).
static QMenu* shared_menu_of_main_window() {
    QMainWindow *mainWin = find_obs_main_window();
    QMenuBar *menuBar = mainWin ? mainWin->menuBar() : nullptr;
    return menuBar ? find_shared_menu(menuBar) : nullptr;
}

// ============================================================================
// C-FFI Exports (Matching Trust obs_source.rs expectations)
// ============================================================================
extern "C" {

__declspec(dllexport) void trust_ui_set_menu_title(const char *menu_title) {
    if (!menu_title || !*menu_title) return;
    g_current_menu_title = menu_title;
    qt_log("[trust_qt] trust_ui_set_menu_title: '%s'", menu_title);

    QMainWindow *mainWin = find_obs_main_window();
    if (!mainWin) {
        qt_log("[trust_qt] ⚠️ find_obs_main_window returned NULL");
        return;
    }
    QMenuBar *menuBar = mainWin->menuBar();
    if (!menuBar) {
        qt_log("[trust_qt] ⚠️ mainWin->menuBar() returned NULL");
        return;
    }

    // The title isn't shown any more: Theater Scene's items go straight into
    // the shared menu, which keeps its own title.
    if (!g_top_menu) {
        g_top_menu = get_shared_menu(menuBar);
        qt_log("[trust_qt] Joined the shared '%s' menu at %p", kSharedMenuTitle, g_top_menu.data());
    }
    menuBar->update();
}

__declspec(dllexport) void trust_ui_clear_menu(void) {
    qt_log("[trust_qt] trust_ui_clear_menu called");
    // Only Theater Scene's own items: the shared menu holds other plugins' too
    if (g_top_menu) {
        start_own_group(g_top_menu);
    }
}

__declspec(dllexport) void trust_ui_add_menu_action(const char *action_title, LuaBtnCallback callback, void *user_data) {
    if (!action_title) return;
    qt_log("[trust_qt] trust_ui_add_menu_action: '%s'", action_title);

    QMainWindow *mainWin = find_obs_main_window();
    if (!mainWin) {
        qt_log("[trust_qt] ⚠️ find_obs_main_window returned NULL in add_menu_action");
        return;
    }
    QMenuBar *menuBar = mainWin->menuBar();
    if (!menuBar) return;

    if (!g_top_menu) {
        g_top_menu = get_shared_menu(menuBar);
        qt_log("[trust_qt] Joined the shared '%s' menu on-demand: %p", kSharedMenuTitle, g_top_menu.data());
    }
    if (g_top_menu) {
        QString actStr = QString::fromUtf8(action_title);
        QAction *act = g_top_menu->addAction(actStr);
        name_own_item(act);
        if (act && !own_icon_path().empty()) {
            act->setIcon(QIcon(QString::fromUtf8(own_icon_path().c_str())));
        }
        qt_log("[trust_qt] Added action '%s' at %p", action_title, act);
        if (callback && act) {
            bool ok = connect_signal_to_callback((QObject*)act, "triggered(bool)", "triggered()", callback, user_data);
            qt_log("[trust_qt] Connected action '%s' signal: ok=%d", action_title, ok);
        }
        menuBar->update();
    }
}

// Any plugin's group in the shared menu, with any icon: the plugin passes its
// id and its icon's path, so a new plugin or icon needs no change here.
// OBS's UI thread only (obs_module_load, or a task queued with OBS_TASK_UI).

// (Re)starts `plugin_id`'s group: its earlier items removed, then its
// separator. False if OBS's menu bar isn't there yet.
__declspec(dllexport) bool trust_ui_menu_start_group(const char *plugin_id) {
    if (!plugin_id || !*plugin_id) return false;
    QMenu *menu = shared_menu_of_main_window();
    if (!menu) {
        qt_log("[trust_qt] ⚠️ No shared menu yet for '%s'", plugin_id);
        return false;
    }
    start_group(menu, plugin_id);
    qt_log("[trust_qt] Started the '%s' group in the shared menu", plugin_id);
    return true;
}

// Adds an item to `plugin_id`'s group, showing the image at `icon_path`
// (UTF-8; null, empty or a missing file: no icon), calling `callback` when
// clicked. Start the group first (trust_ui_menu_start_group).
__declspec(dllexport) bool trust_ui_menu_add_item(const char *plugin_id, const char *icon_path, const char *title,
                                                  LuaBtnCallback callback, void *user_data) {
    if (!plugin_id || !*plugin_id || !title) return false;
    QMenu *menu = shared_menu_of_main_window();
    if (!menu) return false;
    QAction *act = menu->addAction(QString::fromUtf8(title));
    if (!act) return false;
    name_item(act, plugin_id);
    // (A missing file makes an empty QIcon: no icon)
    if (icon_path && *icon_path) {
        act->setIcon(QIcon(QString::fromUtf8(icon_path)));
    }
    if (callback) {
        bool ok = connect_signal_to_callback((QObject*)act, "triggered(bool)", "triggered()", callback, user_data);
        qt_log("[trust_qt] '%s' item '%s' connected: ok=%d", plugin_id, title, ok);
    }
    return true;
}

// Takes `plugin_id`'s items out of the shared menu (for a plugin that unloads
// without pinning its DLL, since its items call into it).
__declspec(dllexport) void trust_ui_menu_remove_group(const char *plugin_id) {
    remove_items(shared_menu_of_main_window(), plugin_id);
}

__declspec(dllexport) uint32_t trust_ui_create_window(const char *window_id, const char *title, int width, int height) {
    const char *wTitle = (title && *title) ? title : "🎬 Window";
    const char *wId = (window_id && *window_id) ? window_id : "custom_window";
    qt_log("[trust_qt] trust_ui_create_window: id='%s', title='%s', %dx%d", wId, wTitle, width, height);

    QMainWindow *mainWin = find_obs_main_window();
    QDialog *win = new QDialog(mainWin, Qt::Window);
    win->setWindowTitle(QString::fromUtf8(wTitle));
    win->setObjectName(QString::fromUtf8(wId));
    win->resize(width > 0 ? width : 640, height > 0 ? height : 480);
    win->setStyleSheet(QString::fromUtf8("QDialog { background-color: #0f172a; color: #f8fafc; font-family: 'Segoe UI', sans-serif; }"));

    QVBoxLayout *layout = new QVBoxLayout(win);
    layout->setContentsMargins(12, 12, 12, 12);
    box_set_spacing(layout, 8);

    uint32_t handle = g_next_ui_handle++;
    g_universal_windows[handle] = win;
    g_universal_containers[handle] = win;
    return handle;
}

__declspec(dllexport) uint32_t trust_ui_create_dock(const char *dock_id, const char *title) {
    const char *dTitle = (title && *title) ? title : "🎬 Dock";
    const char *dId = (dock_id && *dock_id) ? dock_id : "custom_dock";
    qt_log("[trust_qt] trust_ui_create_dock: id='%s', title='%s'", dId, dTitle);

    QMainWindow *mainWin = find_obs_main_window();
    QWidget *dockContent = new QWidget(mainWin);
    QString idStr = QString::fromUtf8(dId);
    dockContent->setObjectName(idStr);

    QVBoxLayout *layout = new QVBoxLayout(dockContent);
    layout->setContentsMargins(10, 10, 10, 10);

    HMODULE obs_fe = GetModuleHandleA("obs-frontend-api.dll");
    if (!obs_fe) obs_fe = LoadLibraryA("obs-frontend-api.dll");
    if (obs_fe) {
        // A custom dock gets no toggle in OBS's Docks menu: it's opened from
        // Theater Scene's own menu (trust_ui_show_dock). OBS still saves and
        // restores its layout by id. Starts hidden and floating, like
        // obs_frontend_add_dock_by_id docks.
        typedef bool (*obs_frontend_add_custom_qdock_t)(const char *id, void *dock);
        auto add_custom_fn = (obs_frontend_add_custom_qdock_t)GetProcAddress(obs_fe, "obs_frontend_add_custom_qdock");
        if (add_custom_fn) {
            QDockWidget *dock = new QDockWidget(mainWin);
            dock->setWindowTitle(QString::fromUtf8(dTitle));
            dock->setWidget(dockContent);
            if (add_custom_fn(dId, dock)) {
                dock->hide();
                dock->setFloating(true);
                qt_log("[trust_qt] obs_frontend_add_custom_qdock called successfully for '%s'", dId);
            } else {
                // OBS already has a dock with that id; the dock (and its content) is ours to delete
                qt_log("[trust_qt] ⚠️ obs_frontend_add_custom_qdock refused dock id '%s' (already used)", dId);
                dock->deleteLater();
            }
        } else {
            qt_log("[trust_qt] ⚠️ obs_frontend_add_custom_qdock symbol not found");
        }
    }

    uint32_t handle = g_next_ui_handle++;
    g_universal_containers[handle] = dockContent;
    return handle;
}

// Shows a native window of Theater's (an egui dock: a window of the Studio's
// thread) inside a dock made by trust_ui_create_dock. Qt's window container
// keeps it the dock's size, docked or floating.
__declspec(dllexport) bool trust_ui_embed_native_window(uint32_t container_handle, uintptr_t native_window) {
    auto it = g_universal_containers.find(container_handle);
    if (it == g_universal_containers.end() || !it->second || !native_window) return false;
    QWidget *content = it->second.data();
    QWindow *window = QWindow::fromWinId((unsigned long long)native_window);
    if (!window) {
        qt_log("[trust_qt] ⚠️ trust_ui_embed_native_window: Qt refused window %p", (void*)native_window);
        return false;
    }
    QWidget *holder = QWidget::createWindowContainer(window, content);
    QBoxLayout *layout = (QBoxLayout*)content->layout();
    if (layout) {
        layout->setContentsMargins(0, 0, 0, 0);
        layout->addWidget(holder, 1);
    }
    holder->show();
    g_native_windows[container_handle] = window;
    qt_log("[trust_qt] Embedded native window %p in dock handle %u", (void*)native_window, container_handle);
    return true;
}

// Gives an embedded window back before OBS takes its docks down: hidden and
// out of the dock, it stays Theater's to close
__declspec(dllexport) void trust_ui_release_native_window(uint32_t container_handle) {
    auto it = g_native_windows.find(container_handle);
    if (it == g_native_windows.end()) return;
    if (QWindow *window = it->second.data()) {
        window->hide();
        window->setParent(nullptr);
        qt_log("[trust_qt] Released the native window of dock handle %u", container_handle);
    }
    g_native_windows.erase(it);
}

__declspec(dllexport) uint32_t trust_ui_add_tab(uint32_t container_handle, const char *title, const char *icon) {
    auto it = g_universal_containers.find(container_handle);
    if (it == g_universal_containers.end() || !it->second) return 0;

    QWidget *parentWidget = it->second.data();
    QLayout *parentLayout = parentWidget->layout();
    if (!parentLayout) return 0;

    // parentWidget->findChild<QTabWidget*>(). QTabWidget::staticMetaObject is a
    // data export, and data can't be imported through /DELAYLOAD (the import
    // slot holds a delay-load stub until the first call), so it's resolved here.
    static auto tabWidgetMeta = (const QMetaObject*)qt_widgets_proc("?staticMetaObject@QTabWidget@@2UQMetaObject@@B");
    QTabWidget *tabHost = tabWidgetMeta ? (QTabWidget*)qt_qFindChild_helper(parentWidget, QString(), *tabWidgetMeta, Qt::FindChildrenRecursively) : nullptr;
    if (!tabHost) {
        tabHost = new QTabWidget(parentWidget);
        tabHost->setStyleSheet(QString::fromUtf8("QTabWidget::pane { border: 1px solid #334155; background: #0f172a; } QTabBar::tab { background: #1e293b; color: #94a3b8; padding: 6px 14px; margin-right: 2px; } QTabBar::tab:selected { background: #0f172a; color: #38bdf8; font-weight: bold; border-top: 2px solid #38bdf8; }"));
        parentLayout->addWidget(tabHost);
    }

    QWidget *tabWidget = new QWidget();
    QVBoxLayout *tabLayout = new QVBoxLayout(tabWidget);
    tabLayout->setContentsMargins(6, 6, 6, 6);
    box_set_spacing(tabLayout, 6);

    QScrollArea *scroll = new QScrollArea(tabWidget);
    scroll->setWidgetResizable(true);
    QWidget *scrollContent = new QWidget();
    QVBoxLayout *scrollLayout = new QVBoxLayout(scrollContent);
    scrollLayout->setContentsMargins(0, 0, 0, 0);
    box_set_spacing(scrollLayout, 6);
    scroll->setWidget(scrollContent);
    tabLayout->addWidget(scroll);

    std::string tabLabel = std::string(icon ? icon : "🎯") + " " + (title ? title : "Tab");
    tabHost->addTab(tabWidget, QString::fromUtf8(tabLabel.c_str()));
    qt_log("[trust_qt] Added tab '%s' to container %u: tabHost=%p", tabLabel.c_str(), container_handle, tabHost);

    uint32_t handle = g_next_ui_handle++;
    g_universal_containers[handle] = scrollContent;
    return handle;
}

__declspec(dllexport) uint32_t trust_ui_add_card(uint32_t container_handle, const char *card_title) {
    auto it = g_universal_containers.find(container_handle);
    if (it == g_universal_containers.end() || !it->second) return 0;

    QWidget *parentWidget = it->second.data();
    QLayout *parentLayout = parentWidget->layout();
    if (!parentLayout) return 0;

    QFrame *card = new QFrame(parentWidget);
    card->setStyleSheet(QString::fromUtf8("QFrame { background: #1e293b; border: 1px solid #334155; border-radius: 6px; padding: 6px; }"));
    QVBoxLayout *cardLayout = new QVBoxLayout(card);
    cardLayout->setContentsMargins(6, 6, 6, 6);
    box_set_spacing(cardLayout, 6);

    if (card_title && *card_title) {
        QLabel *lbl = new QLabel(QString::fromUtf8(card_title), card);
        lbl->setStyleSheet(QString::fromUtf8("font-size: 10px; font-weight: bold; color: #38bdf8; letter-spacing: 0.5px;"));
        cardLayout->addWidget(lbl);
    }

    parentLayout->addWidget(card);
    qt_log("[trust_qt] Added card '%s' to container %u: %p", card_title ? card_title : "", container_handle, card);

    uint32_t handle = g_next_ui_handle++;
    g_universal_containers[handle] = card;
    return handle;
}

__declspec(dllexport) bool trust_ui_add_split_view(uint32_t container_handle, int ratio_left, int ratio_right, uint32_t *out_left, uint32_t *out_right) {
    auto it = g_universal_containers.find(container_handle);
    if (it == g_universal_containers.end() || !it->second) return false;

    QWidget *parentWidget = it->second.data();
    QLayout *parentLayout = parentWidget->layout();
    if (!parentLayout) return false;

    QWidget *splitHost = new QWidget(parentWidget);
    QHBoxLayout *hLayout = new QHBoxLayout(splitHost);
    hLayout->setContentsMargins(0, 0, 0, 0);
    box_set_spacing(hLayout, 8);

    QWidget *leftPanel = new QWidget(splitHost);
    QVBoxLayout *leftLayout = new QVBoxLayout(leftPanel);
    leftLayout->setContentsMargins(0, 0, 0, 0);
    box_set_spacing(leftLayout, 6);

    QWidget *rightPanel = new QWidget(splitHost);
    QVBoxLayout *rightLayout = new QVBoxLayout(rightPanel);
    rightLayout->setContentsMargins(0, 0, 0, 0);
    box_set_spacing(rightLayout, 6);

    hLayout->addWidget(leftPanel, ratio_left > 0 ? ratio_left : 1);
    hLayout->addWidget(rightPanel, ratio_right > 0 ? ratio_right : 1);
    parentLayout->addWidget(splitHost);

    uint32_t handle_l = g_next_ui_handle++;
    uint32_t handle_r = g_next_ui_handle++;
    g_universal_containers[handle_l] = leftPanel;
    g_universal_containers[handle_r] = rightPanel;
    qt_log("[trust_qt] Added split view to container %u: left=%u right=%u", container_handle, handle_l, handle_r);

    if (out_left) *out_left = handle_l;
    if (out_right) *out_right = handle_r;
    return true;
}

__declspec(dllexport) void trust_ui_clear_container(uint32_t container_handle) {
    auto it = g_universal_containers.find(container_handle);
    if (it != g_universal_containers.end() && it->second) {
        QWidget *w = it->second.data();
        if (w && w->layout()) {
            QLayout *layout = w->layout();
            if (!layout->inherits("QBoxLayout")) {
                qt_log("[trust_qt] ⚠️ trust_ui_clear_container: container %u layout is not a QBoxLayout", container_handle);
                return;
            }
            int removed = 0;
            QLayoutItem *child;
            while ((child = box_take_at((QBoxLayout*)layout, 0)) != nullptr) {
                if (QWidget *childWidget = layout_item_widget(child)) {
                    childWidget->hide();
                    childWidget->deleteLater();
                }
                layout_item_delete(child);
                removed++;
            }
            qt_log("[trust_qt] Cleared container %u (%d item(s))", container_handle, removed);
        }
    }
}

__declspec(dllexport) uint32_t trust_ui_add_combobox(uint32_t container_handle, const char *label, const char *items_csv, int initial_idx, LuaComboboxCallback callback, void *user_data) {
    auto it = g_universal_containers.find(container_handle);
    if (it == g_universal_containers.end() || !it->second) return 0;

    QWidget *parentWidget = it->second.data();
    QLayout *parentLayout = parentWidget->layout();
    if (!parentLayout) return 0;

    QWidget *rowWidget = new QWidget(parentWidget);
    QHBoxLayout *row = new QHBoxLayout(rowWidget);
    row->setContentsMargins(0, 0, 0, 0);
    box_set_spacing(row, 8);

    QLabel *lbl = new QLabel(QString::fromUtf8(label ? label : "Select:"), rowWidget);
    lbl->setStyleSheet(QString::fromUtf8("font-weight: bold; color: #94a3b8; font-size: 11px;"));
    QComboBox *combo = new QComboBox(rowWidget);
    combo->setStyleSheet(QString::fromUtf8("QComboBox { background: #0f172a; color: #f8fafc; border: 1px solid #334155; border-radius: 4px; padding: 4px 8px; font-size: 11px; }"));

    if (items_csv && *items_csv) {
        for (const std::string &item : split_csv_trimmed(items_csv)) {
            combo->addItem(QString::fromUtf8(item.c_str(), (int64_t)item.size()));
        }
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

    qt_log("[trust_qt] Added combobox '%s' to container %u: %p (%d item(s))", label ? label : "Select:", container_handle, combo, combo->count());

    if (callback) {
        auto *slot = new QtPrivate::ArgSlotObject(dispatch_combobox, (void*)callback, user_data, combo);
        bool ok = connect_signal_to_slot(combo, "currentIndexChanged(int)", nullptr, slot);
        qt_log("[trust_qt] Connected combobox currentIndexChanged signal: ok=%d", ok);
    }

    return handle;
}

__declspec(dllexport) uint32_t trust_ui_add_button(uint32_t container_handle, const char *label, const char *style_type, LuaBtnCallback callback, void *user_data) {
    auto it = g_universal_containers.find(container_handle);
    if (it == g_universal_containers.end() || !it->second) return 0;

    QWidget *parentWidget = it->second;
    QLayout *parentLayout = parentWidget->layout();
    if (!parentLayout) return 0;

    const char *btnLabel = label ? label : "Button";
    QString btnText = QString::fromUtf8(btnLabel);
    QPushButton *btn = new QPushButton(btnText, parentWidget);
    ((QBoxLayout*)parentLayout)->addWidget(btn);

    qt_log("[trust_qt] Added button '%s' to container %u: %p", btnLabel, container_handle, btn);

    if (callback) {
        bool ok = connect_signal_to_callback((QObject*)btn, "clicked(bool)", "clicked()", callback, user_data);
        qt_log("[trust_qt] Connected button '%s' clicked signal: ok=%d", btnLabel, ok);
    }

    uint32_t handle = g_next_ui_handle++;
    g_universal_containers[handle] = btn;
    return handle;
}

__declspec(dllexport) uint32_t trust_ui_add_checkbox(uint32_t container_handle, const char *label, bool initial_state, LuaCheckCallback callback, void *user_data) {
    auto it = g_universal_containers.find(container_handle);
    if (it == g_universal_containers.end() || !it->second) return 0;

    QWidget *parentWidget = it->second.data();
    QLayout *parentLayout = parentWidget->layout();
    if (!parentLayout) return 0;

    const char *chkLabel = label ? label : "Option";
    QCheckBox *check = new QCheckBox(QString::fromUtf8(chkLabel), parentWidget);
    check->setChecked(initial_state);
    check->setStyleSheet(QString::fromUtf8("color: #e2e8f0; font-size: 11px;"));
    parentLayout->addWidget(check);

    uint32_t handle = g_next_ui_handle++;
    DynamicWidgetNode node;
    node.handle = handle;
    node.widget = check;
    node.checkCb = callback;
    node.userData = user_data;
    g_dynamic_widgets[handle] = node;

    qt_log("[trust_qt] Added checkbox '%s' to container %u: %p", chkLabel, container_handle, check);

    if (callback) {
        auto *slot = new QtPrivate::ArgSlotObject(dispatch_check, (void*)callback, user_data);
        bool ok = connect_signal_to_slot(check, "toggled(bool)", nullptr, slot);
        qt_log("[trust_qt] Connected checkbox '%s' toggled signal: ok=%d", chkLabel, ok);
    }

    return handle;
}

__declspec(dllexport) uint32_t trust_ui_add_table(uint32_t container_handle, const char *headers_csv) {
    auto it = g_universal_containers.find(container_handle);
    if (it == g_universal_containers.end() || !it->second) return 0;

    QWidget *parentWidget = it->second.data();
    QLayout *parentLayout = parentWidget->layout();
    if (!parentLayout) return 0;

    QTableWidget *table = new QTableWidget(parentWidget);
    table->setShowGrid(true);
    table->setStyleSheet(QString::fromUtf8(
        "QTableWidget { background-color: #0b1120; alternate-background-color: #1e293b; color: #f8fafc; gridline-color: #334155; border: 1px solid #334155; border-radius: 6px; font-size: 13px; }"
        "QHeaderView { background-color: #1e293b; border: none; margin: 0px; padding: 0px; }"
        "QHeaderView::section { background-color: #1e293b; color: #38bdf8; font-weight: bold; border: none; border-right: 1px solid #334155; border-bottom: 1px solid #334155; margin: 0px; padding: 0px 4px; height: 32px; font-size: 12px; }"
        "QHeaderView::section:last { border-right: none; }"
        "QTableWidget::item { padding: 4px 8px; border: none; }"
        "QTableWidget::item:selected { background-color: #0284c7; color: #ffffff; }"
    ));
    table->setAlternatingRowColors(true);
    header_set_visible(table->verticalHeader(), false);
    table->horizontalHeader()->setHighlightSections(false);
    table->horizontalHeader()->setDefaultAlignment(Qt::AlignCenter);
    table->setSelectionBehavior(QAbstractItemView::SelectRows);

    std::vector<std::string> headers;
    if (headers_csv && *headers_csv) {
        headers = split_csv_trimmed(headers_csv);
    }
    table->setColumnCount((int)headers.size());
    // setHorizontalHeaderLabels(headers): one header item per column
    for (int i = 0; i < (int)headers.size(); ++i) {
        table->setHorizontalHeaderItem(i, new QTableWidgetItem(QString::fromUtf8(headers[i].c_str(), (int64_t)headers[i].size())));
    }
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
    qt_log("[trust_qt] Added table to container %u: %p (%d column(s))", container_handle, table, (int)headers.size());

    uint32_t handle = g_next_ui_handle++;
    DynamicWidgetNode node;
    node.handle = handle;
    node.widget = table;
    g_dynamic_widgets[handle] = node;
    return handle;
}

__declspec(dllexport) void trust_ui_add_table_row(uint32_t table_handle, const char *name, bool target_checked, bool custom_checked, LuaTableCheckCallback callback, void *user_data) {
    auto it = g_dynamic_widgets.find(table_handle);
    if (it == g_dynamic_widgets.end() || !it->second.widget) return;

    QTableWidget *table = qt_cast<QTableWidget>(it->second.widget.data(), "QTableWidget");
    if (!table) return;

    int row = table->rowCount();
    table->insertRow(row);
    table->setRowHeight(row, 34);

    QTableWidgetItem *nameItem = new QTableWidgetItem(QString::fromUtf8(name ? name : ""));
    nameItem->setFlags(nameItem->flags().toInt() & ~Qt::ItemIsEditable);
    nameItem->setTextAlignment(Qt::AlignLeft | Qt::AlignVCenter);
    nameItem->setForeground(QBrush(QColor("#f8fafc")));
    table->setItem(row, 0, nameItem);

    QWidget *checkWidget1 = new QWidget(table);
    checkWidget1->setStyleSheet(QString::fromUtf8("background: transparent; margin: 0px; padding: 0px;"));
    QHBoxLayout *layout1 = new QHBoxLayout(checkWidget1);
    layout1->setContentsMargins(0, 0, 0, 0);
    box_set_spacing(layout1, 0);
    layout1->setAlignment(Qt::AlignCenter);
    QCheckBox *cb1 = new QCheckBox(checkWidget1);
    cb1->setChecked(target_checked);
    layout1->addWidget(cb1);
    table->setCellWidget(row, 1, checkWidget1);

    QWidget *checkWidget2 = new QWidget(table);
    checkWidget2->setStyleSheet(QString::fromUtf8("background: transparent; margin: 0px; padding: 0px;"));
    QHBoxLayout *layout2 = new QHBoxLayout(checkWidget2);
    layout2->setContentsMargins(0, 0, 0, 0);
    box_set_spacing(layout2, 0);
    layout2->setAlignment(Qt::AlignCenter);
    QCheckBox *cb2 = new QCheckBox(checkWidget2);
    cb2->setChecked(custom_checked);
    layout2->addWidget(cb2);
    table->setCellWidget(row, 2, checkWidget2);

    qt_log("[trust_qt] Added table row %d '%s' to table %u", row, name ? name : "", table_handle);

    if (callback) {
        connect_signal_to_slot(cb1, "toggled(bool)", nullptr, new QtPrivate::ArgSlotObject(dispatch_table_check, (void*)callback, user_data, nullptr, row, 1));
        connect_signal_to_slot(cb2, "toggled(bool)", nullptr, new QtPrivate::ArgSlotObject(dispatch_table_check, (void*)callback, user_data, nullptr, row, 2));
    }
}

__declspec(dllexport) void trust_ui_clear_table(uint32_t table_handle) {
    auto it = g_dynamic_widgets.find(table_handle);
    if (it != g_dynamic_widgets.end() && it->second.widget) {
        if (QTableWidget *table = qt_cast<QTableWidget>(it->second.widget.data(), "QTableWidget")) {
            table->setRowCount(0);
            qt_log("[trust_qt] Cleared table %u", table_handle);
        }
    }
}

__declspec(dllexport) void trust_ui_set_table_select_callback(uint32_t table_handle, LuaTableSelectCallback callback, void *user_data) {
    auto it = g_dynamic_widgets.find(table_handle);
    if (it == g_dynamic_widgets.end() || !it->second.widget) return;

    QTableWidget *table = qt_cast<QTableWidget>(it->second.widget.data(), "QTableWidget");
    if (!table) return;

    if (callback) {
        auto *slot = new QtPrivate::ArgSlotObject(dispatch_table_select, (void*)callback, user_data, table);
        bool ok = connect_signal_to_slot(table, "itemSelectionChanged()", nullptr, slot);
        qt_log("[trust_qt] Connected table %u itemSelectionChanged signal: ok=%d", table_handle, ok);
    }
}

__declspec(dllexport) void trust_ui_show_window(uint32_t window_handle) {
    auto it = g_universal_windows.find(window_handle);
    if (it != g_universal_windows.end() && it->second) {
        QDialog *win = it->second.data();
        if (win) {
            qt_log("[trust_qt] Showing window handle %u: %p", window_handle, win);
            win->show();
            win->raise();
            win->activateWindow();
        }
    }
}

__declspec(dllexport) void trust_ui_show_dock(uint32_t dock_handle) {
    auto cit = g_universal_containers.find(dock_handle);
    if (cit == g_universal_containers.end() || !cit->second) return;

    QMainWindow *mainWin = find_obs_main_window();
    QWidget *w = cit->second;
    QWidget *topDock = w;
    while (w) {
        QWidget *parent = w->parentWidget();
        if (!parent || parent == (QWidget*)mainWin) {
            topDock = w;
            break;
        }
        w = parent;
        topDock = w;
    }
    qt_log("[trust_qt] Showing dock handle %u: targetWidget=%p", dock_handle, topDock);
    if (topDock) {
        topDock->show();
        topDock->raise();
        topDock->activateWindow();
    }
}

__declspec(dllexport) void trust_ui_cleanup(void) {
    for (auto &pair : g_dynamic_widgets) {
        pair.second.sliderCb = nullptr;
        pair.second.btnCb = nullptr;
        pair.second.textCb = nullptr;
        pair.second.checkCb = nullptr;
        if (pair.second.widget) {
            QObject::disconnect(pair.second.widget, nullptr, nullptr, nullptr);
        }
    }
    g_dynamic_widgets.clear();
    g_universal_containers.clear();
    g_universal_windows.clear();

    // The shared menu isn't ours: take out only our items (and their callbacks)
    remove_own_items(g_top_menu);
    g_top_menu = nullptr;
    qt_log("[trust_qt] 🛑 Qt UI Windows toolkit cleaned up");
}

} // extern "C"
