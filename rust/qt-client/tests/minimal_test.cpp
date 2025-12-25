#include <QApplication>
#include <QWidget>
#include <QLabel>
int main(int argc, char **argv) {
    QApplication app(argc, argv);
    QWidget w;
    QLabel *l = new QLabel("hello", &w);
    w.show();
    // don't exec app; just let it construct and destruct
    return 0;
}
