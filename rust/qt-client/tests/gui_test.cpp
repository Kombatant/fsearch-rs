#include <QApplication>
#include <QLineEdit>
#include <QPushButton>
#include <QListWidget>
#include <QLabel>
#include <QVBoxLayout>
#include <QTest>
#include <QTimer>
#include <QThreadPool>
#include <cstdio>
#include <atomic>
#include <unistd.h>
#include <cstdlib>
#include <thread>
#include <chrono>
#include <QPointer>
#include <ctime>
#include <string>

// Custom event for posting results
class AddResultEvent : public QEvent {
public:
    static int eventType() {
        static int t = QEvent::registerEventType();
        return t;
    }
    QString text;
    AddResultEvent(const QString &s) : QEvent((QEvent::Type)eventType()), text(s) {}
};

#include "fsearch_ffi.h"

static std::atomic<int> g_count{0};
static std::atomic<bool> g_shutting_down{false};

static std::string now_ts() {
    using namespace std::chrono;
    auto now = system_clock::now();
    auto tt = system_clock::to_time_t(now);
    struct tm tm;
    gmtime_r(&tt, &tm);
    char buf[64];
    size_t n = strftime(buf, sizeof(buf), "%Y-%m-%dT%H:%M:%S", &tm);
    auto us = duration_cast<microseconds>(now.time_since_epoch()).count() % 1000000;
    char out[96];
    snprintf(out, sizeof(out), "%s.%06lldZ", buf, (long long)us);
    return std::string(out);
}

extern "C" void test_cb(uint64_t id, const char *name, const char *path, uint64_t size, uint64_t mtime, const char *highlights, void *userdata) {
    (void)id; (void)size; (void)mtime; (void)highlights;
    if (g_shutting_down.load(std::memory_order_acquire)) {
        fprintf(stderr, "%s test_cb: ignoring callback during shutdown userdata=%p\n", now_ts().c_str(), userdata);
        return;
    }
    // userdata is now a ResultCollector* (QObject) rather than a raw QListWidget*
    QObject *obj = static_cast<QObject *>(userdata);
    if (!obj) return;
    fprintf(stderr, "%s test_cb: userdata(obj)=%p name=%s\n", now_ts().c_str(), (void*)obj, name ? name : "");
    QString nameStr = QString::fromUtf8(name ? name : "");
    // Use a QPointer guard and post a queued lambda on the application instance
    // Post a custom event directly to the collector object so Qt will drop
    // the event if the object is deleted before delivery.
    if (obj) {
        AddResultEvent *ev = new AddResultEvent(nameStr);
        QCoreApplication::postEvent(obj, ev);
    }
    g_count.fetch_add(1, std::memory_order_relaxed);
}

class ResultCollector : public QObject {
    Q_OBJECT
    QListWidget *m_list;
public:
    explicit ResultCollector(QListWidget *list) : QObject(list), m_list(list) {}
    bool event(QEvent *e) override {
        if (e->type() == AddResultEvent::eventType()) {
            AddResultEvent *ar = static_cast<AddResultEvent*>(e);
            addResult(ar->text);
            return true;
        }
        return QObject::event(e);
    }
public slots:
    void addResult(const QString &s) {
        if (!m_list) return;
        new QListWidgetItem(s, m_list);
        fprintf(stderr, "%s ResultCollector::addResult: added '%s'\n", now_ts().c_str(), s.toUtf8().constData());
    }
};

class GuiTest : public QObject {
    Q_OBJECT
private slots:
    void smoke();
};

void GuiTest::smoke() {
    int argc = 0; char **argv = nullptr;
    class LoggingApplication : public QApplication {
    public:
        using QApplication::QApplication;
        ~LoggingApplication() override {
            fprintf(stderr, "LoggingApplication::~LoggingApplication() start\n");
            const auto ws = QApplication::allWidgets();
            fprintf(stderr, "LoggingApplication: allWidgets count=%d\n", ws.size());
            for (QWidget *wi : ws) fprintf(stderr, "LA WIDGET %p %s parent=%p\n", (void*)wi, wi->metaObject()->className(), (void*)wi->parent());
            QObject *qa = QCoreApplication::instance();
            if (qa) {
                fprintf(stderr, "LoggingApplication: qApp instance=%p\n", (void*)qa);
            }
            fprintf(stderr, "LoggingApplication::~LoggingApplication() end\n");
        }
    } app(argc, argv);

    QWidget *w = new QWidget();
    QVBoxLayout *layout = new QVBoxLayout(w);
    QLineEdit *pathInput = new QLineEdit(w);
    QLineEdit *queryInput = new QLineEdit(w);
    QPushButton *indexBtn = new QPushButton("Build Index", w);
    QPushButton *searchBtn = new QPushButton("Start Search", w);
    QListWidget *resultsList = new QListWidget(w);
    ResultCollector *collector = new ResultCollector(resultsList);
    layout->addWidget(new QLabel("Index paths (comma-separated):"));
    layout->addWidget(pathInput);
    layout->addWidget(new QLabel("Query:"));
    layout->addWidget(queryInput);
    layout->addWidget(indexBtn);
    layout->addWidget(searchBtn);
    layout->addWidget(resultsList);

    // wire buttons similar to main.cpp but simpler
    void *idx = nullptr;
    uint64_t search_handle = 0;

    QObject::connect(indexBtn, &QPushButton::clicked, [&]() {
        const char *paths[1] = { "." };
        idx = fsearch_index_build_from_paths_c(paths, 1);
        QTest::qWait(50);
        if (idx) fsearch_index_list_entries_c(idx, test_cb, collector);
    });

    QObject::connect(searchBtn, &QPushButton::clicked, [&]() {
        resultsList->clear();
        QByteArray qb = queryInput->text().toUtf8();
        search_handle = fsearch_start_search_with_cb_c(qb.constData(), test_cb, collector);
    });

    w->show();

    // If requested, run a NO-FFI simulation: spawn a worker that posts many queued GUI updates
    if (getenv("FSEARCH_NOFFI")) {
        fprintf(stderr, "NOFFI: simulating queued GUI updates\n");
        std::thread worker([collector]() {
            for (int i = 0; i < 200; ++i) {
                std::string s = std::string("sim-") + std::to_string(i);
                if (!collector) continue;
                // Post AddResultEvent to the collector. If the collector is
                // deleted before the event is processed, Qt will discard the event.
                if (collector) {
                    AddResultEvent *ev = new AddResultEvent(QString::fromUtf8(s.c_str()));
                    QCoreApplication::postEvent(collector, ev);
                }
                if ((i & 127) == 0) std::this_thread::sleep_for(std::chrono::milliseconds(1));
            }
        });
        worker.join();
        for (int i = 0; i < 20; ++i) {
            QCoreApplication::processEvents();
            QTest::qWait(5);
        }
        // Dump object state before deleting window (helpful for NOFFI runs)
        fprintf(stderr, "--- NOFFI dump: QApplication::allWidgets() ---\n");
        const auto widgets_noffi = QApplication::allWidgets();
        for (QWidget *wi : widgets_noffi) fprintf(stderr, "WIDGET %p %s parent=%p\n", (void*)wi, wi->metaObject()->className(), (void*)wi->parent());
        fprintf(stderr, "--- NOFFI dump: top-level QObject children of qApp ---\n");
        QObject *qa_noffi = QCoreApplication::instance();
        if (qa_noffi) {
            // reuse same dump lambda defined later? define small local dump
            std::function<void(QObject*,int)> dumpObj = [&](QObject *o, int depth) {
                for (int i = 0; i < depth; ++i) fprintf(stderr, "  ");
                const char *cn = o->metaObject() ? o->metaObject()->className() : "(no meta)";
                fprintf(stderr, "OBJ %p %s parent=%p thread=%p\n", (void*)o, cn, (void*)o->parent(), (void*)o->thread());
                const QObjectList children = o->children();
                for (QObject *c : children) dumpObj(c, depth + 1);
            };
            dumpObj(qa_noffi, 0);
        }
        fprintf(stderr, "NOFFI: deleting main window\n");
        delete w;
        w = nullptr;
        QCoreApplication::processEvents();
        return;
    }

    // simulate user: build index, then perform search
    fprintf(stderr, "%s smoke: clicking index button\n", now_ts().c_str());
    QTest::mouseClick(indexBtn, Qt::LeftButton);
    QTest::qWait(200);
    fprintf(stderr, "%s smoke: after index wait idx=%p\n", now_ts().c_str(), (void*)idx);
    queryInput->setText("test");
    fprintf(stderr, "%s smoke: clicking search button\n", now_ts().c_str());
    QTest::mouseClick(searchBtn, Qt::LeftButton);
    QTest::qWait(50);
    fprintf(stderr, "%s smoke: after search click handle=%llu\n", now_ts().c_str(), (unsigned long long)search_handle);

    // reset global counter and wait for results (up to 5s)
    g_count.store(0);
    for (int i = 0; i < 50 && g_count.load() == 0; ++i) QTest::qWait(100);

    QCOMPARE(g_count.load() > 0, true);

    // Quick cleanup: cancel any in-flight search, wait briefly, then free the index.
    if (search_handle != 0) {
        fsearch_cancel_search_c(search_handle);
        // allow background threads a moment to observe cancellation and finish
        for (int i = 0; i < 10; ++i) QTest::qWait(50);
    }

    if (idx) {
        fsearch_index_free(idx);
        idx = nullptr;
    }

    // Drain posted events so any queued callbacks invoked via
    // QMetaObject::invokeMethod(..., Qt::QueuedConnection) are handled
    // before QApplication teardown.
    for (int i = 0; i < 5; ++i) {
        fprintf(stderr, "drain loop %d\n", i);
        QCoreApplication::processEvents();
        QTest::qWait(10);
    }

    // Perform a proper shutdown: prevent late callbacks, then cancel/join searches and clear global state
    fprintf(stderr, "setting g_shutting_down=true and calling fsearch_shutdown()\n");
    g_shutting_down.store(true, std::memory_order_release);
    fsearch_shutdown();
    fprintf(stderr, "fsearch_shutdown() returned\n");

    // allow a brief moment for any final queued events to be processed (1s)
    // give the global Qt thread pool a chance to finish any tasks
    fprintf(stderr, "waiting for QThreadPool global instance to finish\n");
    QThreadPool::globalInstance()->waitForDone(1000);

    // flush any posted AddResultEvent events explicitly
    fprintf(stderr, "sending posted AddResultEvent entries\n");
    QCoreApplication::sendPostedEvents(nullptr, AddResultEvent::eventType());

    for (int i = 0; i < 50; ++i) {
        if ((i & 7) == 0) fprintf(stderr, "post-shutdown drain %d\n", i);
        QCoreApplication::processEvents(QEventLoop::AllEvents);
        QTest::qWait(10);
    }

    // Explicitly delete the main window and its children before QApplication teardown
    // Diagnostic dump: list top-level widgets and QObject tree before delete
    auto dumpObject = [](QObject *o, int depth = 0) {
        for (int i = 0; i < depth; ++i) fprintf(stderr, "  ");
        const char *cn = o->metaObject() ? o->metaObject()->className() : "(no meta)";
        fprintf(stderr, "OBJ %p %s parent=%p thread=%p\n", (void*)o, cn, (void*)o->parent(), (void*)o->thread());
        const QObjectList children = o->children();
        for (QObject *c : children) dumpObject(c, depth + 1);
    };

    fprintf(stderr, "--- dump: QApplication::allWidgets() ---\n");
    const auto widgets = QApplication::allWidgets();
    for (QWidget *wi : widgets) fprintf(stderr, "WIDGET %p %s parent=%p\n", (void*)wi, wi->metaObject()->className(), (void*)wi->parent());
    fprintf(stderr, "--- dump: top-level QObject children of qApp ---\n");
    QObject *qa = QCoreApplication::instance();
    if (qa) dumpObject(qa, 0);

    fprintf(stderr, "deleting main window and children\n");
    delete w;
    w = nullptr;
    QCoreApplication::processEvents();

    // Dump again after delete to see what remains
    fprintf(stderr, "--- dump after delete: QApplication::allWidgets() ---\n");
    const auto widgets2 = QApplication::allWidgets();
    for (QWidget *wi : widgets2) fprintf(stderr, "WIDGET %p %s parent=%p\n", (void*)wi, wi->metaObject()->className(), (void*)wi->parent());
    fprintf(stderr, "--- dump after delete: top-level QObject children of qApp ---\n");
    if (qa) dumpObject(qa, 0);
}

QTEST_MAIN(GuiTest)
#include "gui_test.moc"
