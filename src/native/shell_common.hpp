// Shell extensions remain in this disposable STA process, never the GPUI process.
#define NOMINMAX
#include <windows.h>
#include <shlobj.h>
#include <shobjidl.h>
#include <cstdio>
#include <iostream>
#include <string>
#include <vector>
#include <array>
#include <stdexcept>
IContextMenu2*menu2=nullptr;IContextMenu3*menu3=nullptr;
LRESULT CALLBACK proc(HWND h,UINT m,WPARAM w,LPARAM l){LRESULT result=0;if(menu3&&(m==WM_INITMENUPOPUP||m==WM_UNINITMENUPOPUP||m==WM_DRAWITEM||m==WM_MEASUREITEM||m==WM_MENUCHAR)&&SUCCEEDED(menu3->HandleMenuMsg2(m,w,l,&result)))return result;if(menu2&&(m==WM_INITMENUPOPUP||m==WM_DRAWITEM||m==WM_MEASUREITEM||m==WM_MENUCHAR)&&SUCCEEDED(menu2->HandleMenuMsg(m,w,l)))return 0;return DefWindowProcW(h,m,w,l);}
void require(bool value){if(!value)throw std::runtime_error("shell");}
void exact(HANDLE h,void*data,DWORD n){DWORD count=0;require(ReadFile(h,data,n,&count,nullptr)&&count==n);}
std::wstring text(HANDLE h){uint32_t n=0;exact(h,&n,4);require(n>0&&n<32760);std::wstring s(n,0);exact(h,s.data(),n*2);require(s.find(L'\0')==std::wstring::npos);return s;}
std::array<unsigned char,33> identity(const std::wstring&p){HANDLE h=CreateFileW(p.c_str(),FILE_READ_ATTRIBUTES,FILE_SHARE_READ|FILE_SHARE_WRITE|FILE_SHARE_DELETE,nullptr,OPEN_EXISTING,FILE_FLAG_BACKUP_SEMANTICS|FILE_FLAG_OPEN_REPARSE_POINT,nullptr);require(h!=INVALID_HANDLE_VALUE);BY_HANDLE_FILE_INFORMATION b{};FILE_ID_INFO v{};std::array<unsigned char,33>r{};bool ok=GetFileInformationByHandle(h,&b);if(GetFileInformationByHandleEx(h,FileIdInfo,&v,sizeof(v))){r[0]=1;memcpy(r.data()+1,&v.VolumeSerialNumber,8);memcpy(r.data()+9,v.FileId.Identifier,16);}else{uint64_t volume=b.dwVolumeSerialNumber,id=((uint64_t)b.nFileIndexHigh<<32)|b.nFileIndexLow;memcpy(r.data()+1,&volume,8);memcpy(r.data()+9,&id,8);}CloseHandle(h);require(ok);uint64_t created=((uint64_t)b.ftCreationTime.dwHighDateTime<<32)|b.ftCreationTime.dwLowDateTime;memcpy(r.data()+25,&created,8);return r;}
// Check the filesystem object represented by the parsed PIDL, not just its spelling.
PIDLIST_ABSOLUTE parse(const std::wstring& path,const std::array<unsigned char,33>& expected){
 PIDLIST_ABSOLUTE pidl=nullptr;require(SUCCEEDED(SHParseDisplayName(path.c_str(),nullptr,&pidl,0,nullptr)));
 IShellItem* item=nullptr;PWSTR resolved=nullptr;
 HRESULT hr=SHCreateItemFromIDList(pidl,IID_PPV_ARGS(&item));
 if(SUCCEEDED(hr))hr=item->GetDisplayName(SIGDN_FILESYSPATH,&resolved);
 bool same=false;try{same=SUCCEEDED(hr)&&resolved&&identity(resolved)==expected;}catch(...){}
 if(resolved)CoTaskMemFree(resolved);if(item)item->Release();
 if(!same){CoTaskMemFree(pidl);require(false);}return pidl;
}
