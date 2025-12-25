#include <QApplication>
#include <QLineEdit>
#include <QPushButton>
#include <QListWidget>
#include <QLabel>
#include <QVBoxLayout>
#include <QTest>
#include <QTimer>
#include <atomic>
#include <unistd.h>

#include "fsearch_ffi.h"

static std::atomic<int> g_count{0};

extern "C" void test_cb(uint64_t id, const char *name, const char *path, uint64_t size, uint64_t mtime, const char *highlights, void *userdata) {
    (void)id; (void)size; (void)mtime; (void)highlights;
    QListWidget *list = static_cast<QListWidget *>(userdata);
    if (!list) return;
    QString nameStr = QString::fromUtf8(name ? name : "");
    QMetaObject::invokeMethod(QApplication::instance(), [list, nameStr]() {
        QListWidgetItem *it = new QListWidgetItem(nameStr, list);
        list->addItem(it);
    }, Qt::QueuedConnection);
    g_count.fetch_add(1, std::memory_order_relaxed);
}

class GuiTest : public QObject {
    Q_OBJECT
private slots:
    void smoke();
};

void GuiTest::smoke() {
    int argc = 0; char **argv = nullptr;
    QApplication app(argc, argv);

    QWidget w;
    QVBoxLayout *layout = new QVBoxLayout(&w);
    QLineEdit *pathInput = new QLineEdit(&w);
    QLineEdit *queryInput = new QLineEdit(&w);
    QPushButton *indexBtn = new QPushButton("Build Index", &w);
    QPushButton *searchBtn = new QPushButton("Start Search", &w);
    QListWidget *resultsList = new QListWidget(&w);
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
        if (idx) fsearch_index_list_entries_c(idx, test_cb, resultsList);
    });

    QObject::connect(searchBtn, &QPushButton::clicked, [&]() {
        resultsList->clear();
        QByteArray qb = queryInput->text().toUtf8();
        search_handle = fsearch_start_search_with_cb_c(qb.constData(), test_cb, resultsList);
    });

    w.show();

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
        QCoreApplication::processEvents();
        QTest::qWait(10);
    }

    // Perform a proper shutdown: cancel/join any active searches and clear global state
    fsearch_shutdown();

    // allow a brief moment for any final queued events to be processed (1s)
    for (int i = 0; i < 100; ++i) {
        QCoreApplication::processEvents();
        QTest::qWait(10);
    }
}

QTEST_MAIN(GuiTest)
#include "gui_test.moc"
