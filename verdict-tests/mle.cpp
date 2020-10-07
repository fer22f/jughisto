#include <bits/stdc++.h>
using namespace std;

int main() {
    unsigned char* m = (unsigned char*) malloc(1e9);
    for (int i = 0; i < 1e9; i++) {
        m[i] = i;
    }
    for (int i = 0; i < 1e9; i++) {
        cout << m[i] << "\n";
    }
}
