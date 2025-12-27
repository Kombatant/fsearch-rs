#include <QApplication>
#include <QCoreApplication>
#include <QEvent>
#include <QWidget>
#include <QObject>
#include <QString>
#include <thread>
#include <chrono>
#include <cstdio>

class AddResultEvent : public QEvent {
public:
    static int eventType() {
        static int t = QEvent::registerEventType();
        return t;
    }
    QString text;
    AddResultEvent(const QString &s) : QEvent((QEvent::Type)eventType()), text(s) {}
};

class Receiver : public QObject {
public:
    Receiver(QObject *parent=nullptr) : QObject(parent) {}
    bool event(QEvent *e) override {
        if (e->type() == AddResultEvent::eventType()) {
            // do minimal work
            // fprintf(stderr, "Receiver got event\n");
            return true;
        }
        return QObject::event(e);
    }
};

int main(int argc, char **argv) {
    QApplication app(argc, argv);
    QWidget *w = new QWidget();
    w->show();
    Receiver *r = new Receiver(w);

    const int events = 20000;
    std::thread t([r, events]() {
        for (int i = 0; i < events; ++i) {
            AddResultEvent *ev = new AddResultEvent(QString::fromUtf8("sim"));
            QCoreApplication::postEvent(r, ev);
            if ((i & 511) == 0) std::this_thread::sleep_for(std::chrono::microseconds(50));
        }
    });

    // Give the worker a short moment, then delete the receiver to simulate teardown
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
    fprintf(stderr, "Deleting receiver\n");
    delete r;
    r = nullptr;

    t.join();

    // Let Qt exit (QGuiApplication destructor will run when app goes out of scope)
    fprintf(stderr, "Exiting main (app destructor next)\n");
    return 0;
}

// no moc required
