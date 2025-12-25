#include <QApplication>
#include <QLineEdit>
#include <QPushButton>
#include <QListWidget>
#include <QLabel>
#include <QVBoxLayout>
#include <QTest>
#include <QTimer>
#include <cstdio>
#include <atomic>
#include <unistd.h>
#include <cstdlib>
#include <thread>
#include <chrono>

#include "fsearch_ffi.h"

static std::atomic<int> g_count{0};

extern "C" void test_cb(uint64_t id, const char *name, const char *path, uint64_t size, uint64_t mtime, const char *highlights, void *userdata) {
    (void)id; (void)size; (void)mtime; (void)highlights;
    // userdata is now a ResultCollector* (QObject) rather than a raw QListWidget*
    QObject *obj = static_cast<QObject *>(userdata);
    if (!obj) return;
    fprintf(stderr, "test_cb: userdata(obj)=%p name=%s\n", (void*)obj, name ? name : "");
    QString nameStr = QString::fromUtf8(name ? name : "");
    // Use invokeMethod on the collector object to marshal the QString safely to the GUI thread
    QMetaObject::invokeMethod(obj, "addResult", Qt::QueuedConnection, Q_ARG(QString, nameStr));
    g_count.fetch_add(1, std::memory_order_relaxed);
}

class ResultCollector : public QObject {
    Q_OBJECT
    QListWidget *m_list;
public:
    explicit ResultCollector(QListWidget *list) : QObject(list), m_list(list) {}
public slots:
    void addResult(const QString &s) {
        if (!m_list) return;
        new QListWidgetItem(s, m_list);
        fprintf(stderr, "ResultCollector::addResult: added '%s'\n", s.toUtf8().constData());
    }
};

class GuiTest : public QObject {
    Q_OBJECT
private slots:
    void smoke();
};

void GuiTest::smoke() {
    int argc = 0; char **argv = nullptr;
    QApplication app(argc, argv);

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
            for (int i = 0; i < 5000; ++i) {
                std::string s = std::string("sim-") + std::to_string(i);
                QMetaObject::invokeMethod(QApplication::instance(), [s, collector]() {
                    collector->addResult(QString::fromUtf8(s.c_str()));
                }, Qt::QueuedConnection);
                if ((i & 127) == 0) std::this_thread::sleep_for(std::chrono::milliseconds(1));
            }
        });
        worker.join();
        for (int i = 0; i < 200; ++i) {
            QCoreApplication::processEvents();
            QTest::qWait(5);
        }
        fprintf(stderr, "NOFFI: deleting main window\n");
        delete w;
        w = nullptr;
        QCoreApplication::processEvents();
        return;
    }

    // simulate user: build index, then perform search
    QTest::mouseClick(indexBtn, Qt::LeftButton);
    QTest::qWait(200);
    queryInput->setText("test");
    QTest::mouseClick(searchBtn, Qt::LeftButton);

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
    for (int i = 0; i < 20; ++i) {
        fprintf(stderr, "drain loop %d\n", i);
        QCoreApplication::processEvents();
        QTest::qWait(10);
    }

    // Perform a proper shutdown: cancel/join any active searches and clear global state
    fsearch_shutdown();

    // allow a brief moment for any final queued events to be processed (1s)
    for (int i = 0; i < 100; ++i) {
        if ((i & 15) == 0) fprintf(stderr, "post-shutdown drain %d\n", i);
        QCoreApplication::processEvents();
        QTest::qWait(10);
    }

    // Explicitly delete the main window and its children before QApplication teardown
    fprintf(stderr, "deleting main window and children\n");
    delete w;
    w = nullptr;
    QCoreApplication::processEvents();
}

QTEST_MAIN(GuiTest)
#include "gui_test.moc"
